//! Examen d'un paquet YunoHost deja publie.
//!
//! Sept cents applications sont deja au catalogue, et une bonne part d'entre
//! elles n'atteint pas le niveau 8. Repartir de zero pour les refaire serait
//! du gachis : elles marchent, quelqu'un s'en occupe, et leur historique vaut
//! quelque chose. Ce qui manque, c'est de savoir *ce qui* leur manque.
//!
//! Ce module lit un paquet existant et dit, point par point, ce qui le separe
//! du niveau 8. Chaque constat porte le niveau qu'il bloque, pour qu'on sache
//! par quoi commencer.
//!
//! Il ne juge pas le travail de qui l'a ecrit : les regles du catalogue ont
//! change plusieurs fois, et un paquet de 2021 respecte les regles de 2021.

use serde::{Deserialize, Serialize};
use std::path::Path;
use ynp_core::finding::{Finding, Severity};

/// Ce que l'examen d'un paquet existant a trouve.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Audit {
    /// Identifiant lu dans le manifest.
    pub app: String,
    /// Niveau actuel au catalogue, quand on le connait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub niveau_actuel: Option<u8>,
    /// Format de paquet declare : 1 ou 2.
    #[serde(default)]
    pub format: u8,
    pub constats: Vec<Constat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Constat {
    pub id: String,
    /// Ce qui ne va pas, dit simplement.
    pub quoi: String,
    /// Ce qu'il faut faire.
    pub remede: String,
    /// Niveau du catalogue que ce point empeche d'atteindre. `None` quand il
    /// n'empeche rien mais reste une amelioration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bloque_le_niveau: Option<u8>,
    /// Vrai si yunopack sait ecrire le correctif lui-meme.
    pub reparable: bool,
}

impl Audit {
    /// Le niveau que le paquet peut esperer une fois tout corrige.
    ///
    /// C'est le plus bas des niveaux bloques, moins un — au-dela, rien de ce
    /// qu'on sait lire ne s'y oppose. On ne promet pas mieux que 7 : le
    /// niveau 8 demande une maintenance effective dans le temps, que seule la
    /// chaine d'integration constate.
    pub fn niveau_atteignable(&self) -> u8 {
        let plancher = self
            .constats
            .iter()
            .filter_map(|c| c.bloque_le_niveau)
            .min();
        match plancher {
            Some(n) => n.saturating_sub(1),
            None => 7,
        }
    }

    /// Ce que yunopack sait corriger tout seul.
    pub fn reparables(&self) -> Vec<&Constat> {
        self.constats.iter().filter(|c| c.reparable).collect()
    }

    /// Vrai s'il n'y a rien a redire de ce qu'on sait lire.
    pub fn rien_a_signaler(&self) -> bool {
        self.constats.is_empty()
    }
}

/// Examine un paquet deja ecrit.
///
/// Contrairement a `verify`, qui valide ce que nous venons de produire, cet
/// examen part d'un paquet qu'on n'a pas ecrit : il ne dispose d'aucune
/// specification, et doit tout deduire des fichiers.
pub fn examiner(racine: &Path, niveau_actuel: Option<u8>) -> Audit {
    let manifest = lire(racine, "manifest.toml");
    let format = manifest
        .as_deref()
        .and_then(|m| toml::from_str::<toml::Value>(m).ok())
        .and_then(|v| v.get("packaging_format").and_then(|f| f.as_integer()))
        .unwrap_or(0) as u8;

    let app = manifest
        .as_deref()
        .and_then(|m| toml::from_str::<toml::Value>(m).ok())
        .and_then(|v| {
            v.get("id")
                .and_then(|i| i.as_str())
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_default();

    let mut constats = Vec::new();
    constats.extend(format_du_paquet(racine, format, manifest.as_deref()));
    constats.extend(helpers(racine));
    constats.extend(fichiers_attendus(racine));
    constats.extend(mise_a_jour_automatique(manifest.as_deref()));
    constats.extend(documentation(racine));

    // Le plus bloquant d'abord : c'est par la qu'il faut commencer.
    constats.sort_by_key(|c| (c.bloque_le_niveau.unwrap_or(9), c.id.clone()));

    Audit {
        app,
        niveau_actuel,
        format,
        constats,
    }
}

fn lire(racine: &Path, chemin: &str) -> Option<String> {
    std::fs::read_to_string(racine.join(chemin)).ok()
}

/// Le format 1 est abandonne : un paquet qui l'utilise encore ne beneficie
/// d'aucune des ressources declaratives, et son installation reste artisanale.
fn format_du_paquet(racine: &Path, format: u8, manifest: Option<&str>) -> Vec<Constat> {
    let mut out = Vec::new();
    if manifest.is_none() {
        // Un `manifest.json` signale un paquet reste au format 1.
        let ancien = racine.join("manifest.json").exists();
        out.push(Constat {
            id: "FMT001".into(),
            quoi: if ancien {
                "Le paquet est reste a l'ancien format, abandonne depuis YunoHost 11.".into()
            } else {
                "Aucun manifest.toml : le paquet n'est pas lisible.".into()
            },
            remede: "Passer au format 2 : les ressources declaratives remplacent la moitie \
                     des scripts."
                .into(),
            bloque_le_niveau: Some(1),
            reparable: false,
        });
        return out;
    }
    if format < 2 {
        out.push(Constat {
            id: "FMT002".into(),
            quoi: format!(
                "Le paquet declare le format {format}, alors que le format 2 est le seul maintenu."
            ),
            remede: "Declarer `packaging_format = 2` et remplacer les scripts par les \
                     ressources correspondantes."
                .into(),
            bloque_le_niveau: Some(5),
            reparable: false,
        });
    }
    out
}

/// Les helpers ont ete renommes avec la version 2.1. Les anciens noms
/// fonctionnent encore mais le linter officiel les refuse, ce qui plafonne le
/// niveau.
fn helpers(racine: &Path) -> Vec<Constat> {
    const RENOMMES: &[(&str, &str)] = &[
        ("ynh_add_nginx_config", "ynh_config_add_nginx"),
        ("ynh_add_systemd_config", "ynh_config_add_systemd"),
        ("ynh_add_fpm_config", "ynh_config_add_phpfpm"),
        ("ynh_systemd_action", "ynh_systemctl"),
        ("ynh_replace_string", "ynh_replace"),
        ("ynh_secure_remove", "ynh_safe_rm"),
        ("ynh_print_info", "ynh_print_info"),
        ("ynh_exec_warn_less", "ynh_hide_warnings"),
        ("ynh_restore_file", "ynh_restore"),
    ];

    let Ok(entrees) = std::fs::read_dir(racine.join("scripts")) else {
        return Vec::new();
    };
    let mut trouves: Vec<String> = Vec::new();
    for e in entrees.filter_map(Result::ok) {
        let Ok(contenu) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        for (ancien, neuf) in RENOMMES {
            if ancien != neuf && contenu.contains(ancien) && !trouves.iter().any(|t| t == ancien) {
                trouves.push((*ancien).to_string());
            }
        }
    }
    if trouves.is_empty() {
        return Vec::new();
    }
    trouves.sort();
    vec![Constat {
        id: "HLP001".into(),
        quoi: format!(
            "{} helper(s) portent encore leur ancien nom : {}.",
            trouves.len(),
            trouves.join(", ")
        ),
        remede: "Les renommer selon la version 2.1 des helpers. Le comportement est \
                 identique, seul le nom change."
            .into(),
        bloque_le_niveau: Some(5),
        reparable: true,
    }]
}

/// Ce qu'un paquet doit fournir pour que la chaine d'integration puisse le
/// mener au bout de ses tests.
fn fichiers_attendus(racine: &Path) -> Vec<Constat> {
    let mut out = Vec::new();
    for (chemin, id, quoi, remede, niveau) in [
        (
            "scripts/backup",
            "FIC001",
            "Aucun script de sauvegarde.",
            "Sans lui, la sauvegarde et la restauration ne sont pas verifiables.",
            4u8,
        ),
        (
            "scripts/restore",
            "FIC002",
            "Aucun script de restauration.",
            "Il va de pair avec la sauvegarde ; l'un sans l'autre ne sert a rien.",
            4,
        ),
        (
            "scripts/upgrade",
            "FIC003",
            "Aucun script de mise a jour.",
            "L'application ne pourra jamais etre mise a jour depuis l'administration.",
            4,
        ),
        (
            "tests.toml",
            "FIC004",
            "Aucun fichier de tests.",
            "Il indique a la chaine d'integration quels scenarios jouer.",
            7,
        ),
        (
            "doc/DESCRIPTION.md",
            "FIC005",
            "Aucune description longue.",
            "C'est le texte que voit l'utilisateur avant d'installer.",
            7,
        ),
    ] {
        if !racine.join(chemin).exists() {
            out.push(Constat {
                id: id.into(),
                quoi: quoi.into(),
                remede: remede.into(),
                bloque_le_niveau: Some(niveau),
                reparable: matches!(id, "FIC004" | "FIC005"),
            });
        }
    }
    out
}

/// Sans mise a jour automatique, quelqu'un doit suivre les versions a la main
/// — et c'est ce qui finit par laisser un paquet en arriere.
fn mise_a_jour_automatique(manifest: Option<&str>) -> Vec<Constat> {
    let Some(m) = manifest else {
        return Vec::new();
    };
    if m.contains("autoupdate") {
        return Vec::new();
    }
    vec![Constat {
        id: "MAJ001".into(),
        quoi: "Le paquet ne se met pas a jour tout seul quand une nouvelle version parait.".into(),
        remede: "Declarer `autoupdate.strategy` dans les sources : un robot proposera \
                 alors les montees de version."
            .into(),
        bloque_le_niveau: None,
        reparable: true,
    }]
}

fn documentation(racine: &Path) -> Vec<Constat> {
    let mut out = Vec::new();
    if !racine.join("README.md").exists() {
        out.push(Constat {
            id: "DOC001".into(),
            quoi: "Aucun README.".into(),
            remede: "Il se genere a partir du manifest et de la description ; il n'y a pas \
                     a l'ecrire a la main."
                .into(),
            bloque_le_niveau: None,
            reparable: true,
        });
    }
    if !racine.join("LICENSE").exists() && !racine.join("LICENSE.md").exists() {
        out.push(Constat {
            id: "DOC002".into(),
            quoi: "Aucun fichier de licence.".into(),
            remede: "Reprendre la licence du logiciel empaquete.".into(),
            bloque_le_niveau: None,
            reparable: false,
        });
    }
    out
}

/// Traduit les constats de l'examen en `Finding`, pour les afficher comme les
/// autres.
pub fn en_constats(audit: &Audit) -> Vec<Finding> {
    audit
        .constats
        .iter()
        .map(|c| {
            let severite = match c.bloque_le_niveau {
                Some(n) if n <= 4 => Severity::Blocker,
                Some(_) => Severity::Major,
                None => Severity::Minor,
            };
            Finding::new(&c.id, severite, &c.quoi).remediation(&c.remede)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un repertoire de paquet monte pour le test.
    struct Paquet(std::path::PathBuf);

    impl Paquet {
        fn new() -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "ynp-audit-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(p.join("scripts")).unwrap();
            std::fs::create_dir_all(p.join("doc")).unwrap();
            Self(p)
        }
        fn avec(self, chemin: &str, contenu: &str) -> Self {
            let f = self.0.join(chemin);
            if let Some(d) = f.parent() {
                std::fs::create_dir_all(d).unwrap();
            }
            std::fs::write(f, contenu).unwrap();
            self
        }
        fn complet(self) -> Self {
            self.avec(
                "manifest.toml",
                "packaging_format = 2\nid = \"demo\"\n[resources.sources]\nautoupdate.strategy = \"latest_github_release\"\n",
            )
            .avec("scripts/install", "ynh_config_add_nginx\n")
            .avec("scripts/backup", "x")
            .avec("scripts/restore", "x")
            .avec("scripts/upgrade", "x")
            .avec("tests.toml", "x")
            .avec("doc/DESCRIPTION.md", "x")
            .avec("README.md", "x")
            .avec("LICENSE", "x")
        }
    }

    impl Drop for Paquet {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn un_paquet_complet_n_appelle_aucun_constat() {
        let p = Paquet::new().complet();
        let a = examiner(&p.0, Some(8));
        assert!(a.rien_a_signaler(), "{:?}", a.constats);
        assert_eq!(a.app, "demo");
        assert_eq!(a.format, 2);
        assert_eq!(a.niveau_atteignable(), 7);
    }

    #[test]
    fn un_paquet_sans_sauvegarde_plafonne_au_niveau_3() {
        // Le niveau 4 est celui ou la sauvegarde et la restauration sont
        // verifiees : sans elles, impossible d'y pretendre.
        let p = Paquet::new().complet();
        std::fs::remove_file(p.0.join("scripts/backup")).unwrap();
        let a = examiner(&p.0, Some(2));
        assert_eq!(a.niveau_atteignable(), 3);
        assert_eq!(a.constats[0].id, "FIC001");
        assert_eq!(a.constats[0].bloque_le_niveau, Some(4));
    }

    #[test]
    fn les_anciens_noms_de_helpers_sont_reperes_et_reparables() {
        let p = Paquet::new().complet().avec(
            "scripts/install",
            "ynh_add_nginx_config\nynh_systemd_action\n",
        );
        let a = examiner(&p.0, Some(6));
        let c = a.constats.iter().find(|c| c.id == "HLP001").unwrap();
        assert!(c.quoi.contains("ynh_add_nginx_config"));
        assert!(c.quoi.contains("ynh_systemd_action"));
        assert!(c.reparable, "renommer un helper est mecanique");
        assert_eq!(c.bloque_le_niveau, Some(5));
    }

    #[test]
    fn un_helper_au_nom_inchange_n_est_pas_signale() {
        // `ynh_print_info` porte le meme nom avant et apres : le signaler
        // serait une fausse alerte.
        let p = Paquet::new()
            .complet()
            .avec("scripts/install", "ynh_print_info 'bonjour'\n");
        let a = examiner(&p.0, None);
        assert!(a.rien_a_signaler(), "{:?}", a.constats);
    }

    #[test]
    fn l_ancien_format_est_le_premier_constat() {
        let p = Paquet::new().avec("manifest.json", "{}");
        let a = examiner(&p.0, Some(4));
        assert_eq!(a.constats[0].id, "FMT001");
        assert_eq!(a.niveau_atteignable(), 0);
        assert!(a.constats[0].quoi.contains("ancien format"));
    }

    #[test]
    fn l_absence_de_mise_a_jour_automatique_ne_bloque_aucun_niveau() {
        // Ce n'est pas une faute, c'est une charge de maintenance de plus.
        let p = Paquet::new()
            .complet()
            .avec("manifest.toml", "packaging_format = 2\nid = \"demo\"\n");
        let a = examiner(&p.0, Some(8));
        let c = a.constats.iter().find(|c| c.id == "MAJ001").unwrap();
        assert_eq!(c.bloque_le_niveau, None);
        assert_eq!(a.niveau_atteignable(), 7, "rien ne bloque de niveau");
    }

    #[test]
    fn les_constats_reparables_se_distinguent() {
        let p = Paquet::new().complet();
        std::fs::remove_file(p.0.join("tests.toml")).unwrap();
        std::fs::remove_file(p.0.join("LICENSE")).unwrap();
        let a = examiner(&p.0, None);
        let reparables: Vec<&str> = a.reparables().iter().map(|c| c.id.as_str()).collect();
        assert!(reparables.contains(&"FIC004"), "un tests.toml s'ecrit");
        assert!(
            !reparables.contains(&"DOC002"),
            "choisir une licence n'est pas mecanique"
        );
    }

    #[test]
    fn la_severite_suit_le_niveau_bloque() {
        let p = Paquet::new().complet();
        std::fs::remove_file(p.0.join("scripts/restore")).unwrap();
        std::fs::remove_file(p.0.join("tests.toml")).unwrap();
        let f = en_constats(&examiner(&p.0, None));
        let restore = f.iter().find(|f| f.id == "FIC002").unwrap();
        let tests = f.iter().find(|f| f.id == "FIC004").unwrap();
        assert_eq!(restore.severity, Severity::Blocker);
        assert_eq!(tests.severity, Severity::Major);
    }

    #[test]
    fn un_repertoire_vide_le_dit_plutot_que_de_paniquer() {
        let p = Paquet::new();
        let a = examiner(&p.0, None);
        assert_eq!(a.constats[0].id, "FMT001");
        assert!(a.constats[0].quoi.contains("pas lisible"));
    }
}
