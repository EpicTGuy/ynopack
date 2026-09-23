//! Validation dynamique : la gate G3.
//!
//! Le cycle reproduit ici a d'abord ete execute a la main, commande par
//! commande, sur l'instance de test. Il a revele ce qu'aucune verification
//! statique n'aurait montre : un paquet qui s'installe, provisionne tout
//! correctement, et dont le service ne demarre pas.
//!
//! Chaque etape est enregistree avec son verdict et son journal, de sorte
//! qu'un echec dise ou et pourquoi sans qu'il faille rejouer la campagne.

pub mod ssh;

use serde::{Deserialize, Serialize};
use ssh::{Hote, SshError};
use ynp_core::gate::GateOutcome;
use ynp_core::AppSpec;

/// Ou et comment installer pendant le test.
#[derive(Debug, Clone)]
pub struct Cible {
    pub hote: String,
    /// Domaine YunoHost. A defaut, le domaine principal de l'hote est utilise.
    pub domaine: Option<String>,
    pub chemin: String,
    pub permission: String,
    /// Sauter la sauvegarde et la restauration, plus lentes.
    pub rapide: bool,
}

impl Cible {
    pub fn new(hote: impl Into<String>) -> Self {
        Self {
            hote: hote.into(),
            domaine: None,
            chemin: String::new(),
            permission: "all_users".into(),
            rapide: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Etape {
    pub nom: String,
    pub reussie: bool,
    // `default` est indispensable : sans lui, une etape reussie — dont le
    // detail vide n'est pas serialise — devient illisible a la relecture, et
    // le rapport entier est silencieusement perdu.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    pub duree_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rapport {
    pub hote: String,
    pub app: String,
    pub url: String,
    pub etapes: Vec<Etape>,
}

impl Rapport {
    pub fn reussi(&self) -> bool {
        self.etapes.iter().all(|e| e.reussie)
    }

    pub fn premiere_erreur(&self) -> Option<&Etape> {
        self.etapes.iter().find(|e| !e.reussie)
    }

    pub fn gate(&self) -> GateOutcome {
        use ynp_core::finding::{Evidence, Finding, Severity};
        match self.premiere_erreur() {
            None => GateOutcome::Pass,
            Some(e) => GateOutcome::fail(vec![Finding::new(
                "G3",
                Severity::Blocker,
                format!("Etape en echec : {}", e.nom),
            )
            .detail(e.detail.clone())
            .evidence(Evidence::file(format!("{}:{}", self.hote, self.app)))]),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RunnerError {
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error("l'hote n'expose aucun domaine YunoHost")]
    SansDomaine,
}

/// Deroule le cycle complet sur l'hote.
///
/// L'application est toujours desinstallee a la fin, y compris en cas d'echec :
/// une campagne qui laisse des traces rend la suivante ininterpretable.
pub async fn run_g3(
    paquet: &std::path::Path,
    spec: &AppSpec,
    cible: &Cible,
) -> Result<Rapport, RunnerError> {
    let hote = Hote::new(&cible.hote);
    hote.joignable().await?;

    let app = spec.app.id.clone();
    let domaine = match &cible.domaine {
        Some(d) => d.clone(),
        None => {
            let s = hote
                .executer("yunohost domain main-domain --output-as plain")
                .await?;
            s.texte()
                .lines()
                .find(|l| !l.starts_with('#') && !l.trim().is_empty())
                .map(str::to_string)
                .ok_or(RunnerError::SansDomaine)?
        }
    };
    let chemin = if cible.chemin.is_empty() {
        format!("/{app}")
    } else {
        cible.chemin.clone()
    };
    let url = format!("https://{domaine}{chemin}/");

    let mut etapes = Vec::new();
    let distant = format!("/tmp/ynopack/{app}_ynh");

    // Une installation residuelle d'une campagne precedente fausserait tout.
    let _ = hote
        .executer(&format!("yunohost app remove {app} 2>/dev/null"))
        .await;

    let mut chrono = Chrono::new();
    let envoi = hote.envoyer(paquet, &distant).await?;
    etapes.push(chrono.etape("transfert du paquet", envoi.ok(), envoi.diagnostic()));

    if envoi.ok() {
        let args = format!(
            "domain={domaine}&path={chemin}&init_main_permission={}",
            cible.permission
        );
        let install = hote
            .executer(&format!(
                "yunohost app install {distant} --force -a \"{args}\""
            ))
            .await?;
        etapes.push(chrono.etape("installation", install.ok(), install.diagnostic()));

        if install.ok() {
            etapes.extend(verifier_le_service(&hote, spec, &app, &url, &mut chrono).await?);
            if !cible.rapide {
                etapes.extend(sauvegarde_restauration(&hote, &app, &url, &mut chrono).await?);
            }
        }
    }

    // Desinstallation et controle des residus, quoi qu'il soit arrive avant.
    let remove = hote.executer(&format!("yunohost app remove {app}")).await?;
    etapes.push(chrono.etape("desinstallation", remove.ok(), remove.diagnostic()));

    let residus = chercher_les_residus(&hote, spec, &app).await?;
    etapes.push(chrono.etape(
        "absence de residus",
        residus.is_empty(),
        if residus.is_empty() {
            String::new()
        } else {
            format!("restants : {}", residus.join(", "))
        },
    ));

    let _ = hote.executer(&format!("rm -rf {distant}")).await;

    Ok(Rapport {
        hote: cible.hote.clone(),
        app,
        url,
        etapes,
    })
}

async fn verifier_le_service(
    hote: &Hote,
    spec: &AppSpec,
    app: &str,
    url: &str,
    chrono: &mut Chrono,
) -> Result<Vec<Etape>, SshError> {
    let mut etapes = Vec::new();

    if spec.features.systemd {
        // Le service peut mettre un instant a se stabiliser ; « activating »
        // n'est pas un succes, c'est une boucle de redemarrage qui commence.
        let _ = hote.executer("sleep 3").await;
        let etat = hote.executer(&format!("systemctl is-active {app}")).await?;
        let actif = etat.texte() == "active";
        let detail = if actif {
            String::new()
        } else {
            let journal = hote
                .executer(&format!("journalctl -u {app} -n 15 --no-pager -o cat"))
                .await?;
            format!("etat : {}\n{}", etat.texte(), journal.diagnostic())
        };
        etapes.push(chrono.etape("service actif", actif, detail));
    }

    if spec.features.nginx {
        let code = hote
            .executer(&format!("curl -sk -o /dev/null -w '%{{http_code}}' {url}"))
            .await?;
        let c = code.texte().to_string();
        // 3xx est normal : le portail redirige vers l'authentification.
        let servi = c.starts_with('2') || c.starts_with('3');
        etapes.push(chrono.etape(
            "endpoint HTTP",
            servi,
            if servi {
                String::new()
            } else {
                format!("code {c} sur {url}")
            },
        ));
    }
    Ok(etapes)
}

async fn sauvegarde_restauration(
    hote: &Hote,
    app: &str,
    url: &str,
    chrono: &mut Chrono,
) -> Result<Vec<Etape>, SshError> {
    let mut etapes = Vec::new();
    let archive = format!("ynopack_{app}");

    let sauvegarde = hote
        .executer(&format!(
            "yunohost backup create --apps {app} --name {archive}"
        ))
        .await?;
    etapes.push(chrono.etape("sauvegarde", sauvegarde.ok(), sauvegarde.diagnostic()));
    if !sauvegarde.ok() {
        return Ok(etapes);
    }

    let _ = hote.executer(&format!("yunohost app remove {app}")).await;
    let restauration = hote
        .executer(&format!(
            "yunohost backup restore {archive} --apps {app} --force"
        ))
        .await?;
    etapes.push(chrono.etape("restauration", restauration.ok(), restauration.diagnostic()));

    if restauration.ok() {
        let code = hote
            .executer(&format!("curl -sk -o /dev/null -w '%{{http_code}}' {url}"))
            .await?;
        let c = code.texte().to_string();
        let servi = c.starts_with('2') || c.starts_with('3');
        etapes.push(chrono.etape(
            "endpoint apres restauration",
            servi,
            if servi {
                String::new()
            } else {
                format!("code {c}")
            },
        ));
    }

    let _ = hote
        .executer(&format!("yunohost backup delete {archive}"))
        .await;
    Ok(etapes)
}

/// Ce qu'une desinstallation propre ne doit laisser derriere elle.
///
/// C'est le controle qui attrape les scripts `remove` incomplets, defaut le
/// plus frequent des paquets ecrits a la main.
async fn chercher_les_residus(
    hote: &Hote,
    spec: &AppSpec,
    app: &str,
) -> Result<Vec<String>, SshError> {
    let mut restants = Vec::new();
    let mut verifier = |nom: &str, present: bool| {
        if present {
            restants.push(nom.to_string());
        }
    };

    verifier(
        "utilisateur systeme",
        hote.reussit(&format!("getent passwd {app}")).await,
    );
    verifier(
        "repertoire d'installation",
        hote.reussit(&format!("test -d /var/www/{app}")).await,
    );
    verifier(
        "configuration nginx",
        hote.reussit(&format!("ls /etc/nginx/conf.d/*.d/{app}.conf"))
            .await,
    );
    verifier(
        "unite systemd",
        hote.reussit(&format!("test -f /etc/systemd/system/{app}.service"))
            .await,
    );
    verifier(
        "reglages",
        hote.reussit(&format!("test -d /etc/yunohost/apps/{app}"))
            .await,
    );
    verifier(
        "logrotate",
        hote.reussit(&format!("test -f /etc/logrotate.d/{app}"))
            .await,
    );

    match spec.resources.database.manifest_type() {
        Some("postgresql") => verifier(
            "base postgresql",
            hote.reussit(&format!(
                "sudo -u postgres psql -lqt | cut -d'|' -f1 | grep -qw {app}"
            ))
            .await,
        ),
        Some("mysql") => verifier(
            "base mysql",
            hote.reussit(&format!("mysql -N -e 'show databases' | grep -qw {app}"))
                .await,
        ),
        _ => {}
    }
    Ok(restants)
}

/// Mesure la duree de chaque etape, pour reperer ce qui coute cher.
struct Chrono(std::time::Instant);

impl Chrono {
    fn new() -> Self {
        Self(std::time::Instant::now())
    }

    fn etape(&mut self, nom: &str, reussie: bool, detail: String) -> Etape {
        let duree_s = self.0.elapsed().as_secs();
        self.0 = std::time::Instant::now();
        Etape {
            nom: nom.to_string(),
            reussie,
            detail,
            duree_s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn etape(nom: &str, reussie: bool) -> Etape {
        Etape {
            nom: nom.into(),
            reussie,
            detail: String::new(),
            duree_s: 1,
        }
    }

    fn rapport(etapes: Vec<Etape>) -> Rapport {
        Rapport {
            hote: "dell".into(),
            app: "demo".into(),
            url: "https://x/demo/".into(),
            etapes,
        }
    }

    #[test]
    fn un_cycle_sans_faute_passe_la_porte() {
        let r = rapport(vec![
            etape("installation", true),
            etape("service actif", true),
        ]);
        assert!(r.reussi());
        assert!(!r.gate().blocks_pipeline());
    }

    #[test]
    fn la_premiere_etape_en_echec_est_celle_qui_est_rapportee() {
        let r = rapport(vec![
            etape("installation", true),
            etape("service actif", false),
            etape("endpoint HTTP", false),
        ]);
        assert_eq!(r.premiere_erreur().unwrap().nom, "service actif");
        assert!(r.gate().blocks_pipeline());
    }

    #[test]
    fn un_residu_fait_echouer_la_porte_meme_si_tout_le_reste_a_marche() {
        // C'est precisement ce que la gate doit attraper : un paquet qui
        // fonctionne mais ne sait pas se retirer.
        let r = rapport(vec![
            etape("installation", true),
            etape("absence de residus", false),
        ]);
        assert!(!r.reussi());
        assert_eq!(r.premiere_erreur().unwrap().nom, "absence de residus");
    }
}

#[cfg(test)]
mod serialisation {
    use super::*;

    #[test]
    fn un_rapport_fait_un_aller_retour_json_sans_perte() {
        // Le rapport est relu par `publish` pour decider du niveau a publier :
        // une relecture qui echoue fait annoncer un niveau 0 a tort.
        let r = Rapport {
            hote: "dell".into(),
            app: "demo".into(),
            url: "https://x/demo/".into(),
            etapes: vec![
                Etape {
                    nom: "installation".into(),
                    reussie: true,
                    detail: String::new(),
                    duree_s: 12,
                },
                Etape {
                    nom: "service".into(),
                    reussie: false,
                    detail: "journal".into(),
                    duree_s: 3,
                },
            ],
        };
        let texte = serde_json::to_string(&r).unwrap();
        let relu: Rapport = serde_json::from_str(&texte).unwrap();

        assert_eq!(relu.etapes.len(), 2);
        assert_eq!(relu.etapes[0].nom, "installation");
        assert_eq!(relu.premiere_erreur().unwrap().nom, "service");
    }
}
