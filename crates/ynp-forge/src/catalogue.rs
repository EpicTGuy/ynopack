//! Le catalogue officiel de YunoHost.
//!
//! Il porte, pour chaque application admise, le niveau que lui attribue la
//! chaine d'integration continue : de 0, qui ne s'installe pas, a 8, qui
//! s'installe, se met a jour, se sauvegarde et se restaure sans reproche.
//! C'est la seule mesure publique de la qualite d'un paquet, et elle dit aussi
//! bien ce qui est disponible que ce qui l'est mal.
//!
//! Deux modules l'interrogeaient chacun de leur cote pour savoir ce qui etait
//! deja package ; ils partagent desormais cette lecture.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const URL: &str = "https://raw.githubusercontent.com/YunoHost/apps/main/apps.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Paquet {
    pub id: String,
    /// 0 a 8. Absent quand la chaine n'a pas encore rendu de verdict.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub niveau: Option<u8>,
    /// `working`, `inprogress`, `broken`, `notworking`.
    #[serde(default)]
    pub etat: String,
    /// Reserves declarees : dependances non libres, telemetrie, etc.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reserves: Vec<String>,
    #[serde(default)]
    pub categorie: String,
    /// Depot du paquet YunoHost, pas du logiciel lui-meme.
    #[serde(default)]
    pub depot_paquet: String,
    /// Retire du catalogue a cette date.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retire_le: Option<i64>,
}

impl Paquet {
    /// Vrai si le paquet tient ses promesses : installable, a jour,
    /// sauvegardable. C'est le seuil que vise la documentation de YunoHost.
    pub fn solide(&self) -> bool {
        self.niveau.is_some_and(|n| n >= 4) && self.etat == "working"
    }

    /// Ce que le niveau veut dire, en clair.
    pub fn ce_que_dit_le_niveau(&self) -> &'static str {
        match self.niveau {
            None => "pas encore verifie par la chaine d'integration",
            Some(0) => "ne s'installe pas",
            Some(1) => "s'installe, mais rien de plus n'est verifie",
            Some(2) => "s'installe et se desinstalle proprement",
            Some(3) => "se reinstalle apres desinstallation",
            Some(4) => "se met a jour, se sauvegarde et se restaure",
            Some(5) => "respecte les regles d'ecriture des paquets",
            Some(6) => "s'installe aussi en sous-dossier et en multi-instance",
            Some(7) => "passe tous les tests automatiques",
            _ => "maintenu et sans reproche",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Catalogue {
    /// Par identifiant d'application.
    paquets: BTreeMap<String, Paquet>,
    /// Cle de rapprochement (identifiant, ou `proprietaire/depot` du logiciel
    /// amont) -> identifiant. Les deux catalogues ne nomment pas toujours
    /// pareil, d'ou ce double index.
    index: BTreeMap<String, String>,
}

impl Catalogue {
    pub async fn charger(client: &reqwest::Client) -> Result<Self, reqwest::Error> {
        let brut = client.get(URL).send().await?.text().await?;
        Ok(Self::lire(&brut))
    }

    /// Analyse le TOML du catalogue. Separe du reseau pour etre testable.
    pub fn lire(toml_brut: &str) -> Self {
        let table: BTreeMap<String, toml::Value> = toml::from_str(toml_brut).unwrap_or_default();
        let mut paquets = BTreeMap::new();
        let mut index = BTreeMap::new();

        for (id, entree) in table {
            let depot_paquet = entree
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or_default()
                .to_string();

            index.insert(id.to_lowercase(), id.clone());
            if !depot_paquet.is_empty() {
                // `foo_ynh` sur la forge correspond au logiciel `foo`.
                index.insert(
                    normaliser(depot_paquet.trim_end_matches("_ynh")),
                    id.clone(),
                );
            }

            paquets.insert(
                id.clone(),
                Paquet {
                    niveau: entree
                        .get("level")
                        .and_then(|l| l.as_integer())
                        .map(|l| l.clamp(0, 8) as u8),
                    etat: entree
                        .get("state")
                        .and_then(|s| s.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    reserves: entree
                        .get("antifeatures")
                        .and_then(|a| a.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    categorie: entree
                        .get("category")
                        .and_then(|c| c.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    retire_le: entree.get("deprecated_date").and_then(|d| d.as_integer()),
                    depot_paquet,
                    id,
                },
            );
        }
        Self { paquets, index }
    }

    /// Le paquet correspondant a un identifiant ou a une URL de depot amont.
    pub fn trouver(&self, cle: &str) -> Option<&Paquet> {
        let direct = self.index.get(&cle.to_lowercase());
        let par_depot = self.index.get(&normaliser(cle));
        let id = direct.or(par_depot)?;
        self.paquets.get(id)
    }

    pub fn nombre(&self) -> usize {
        self.paquets.len()
    }
}

/// `proprietaire/depot` en minuscules, pour comparer deux URL de forge.
fn normaliser(url: &str) -> String {
    let parties: Vec<&str> = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect();
    parties
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait reel du catalogue.
    const TOML: &str = r#"
[13ft]
branch = "master"
category = "reading"
level = 8
state = "working"
url = "https://github.com/YunoHost-Apps/13ft_ynh"

[20euros]
antifeatures = [ "non-free-assets" ]
category = "games"
level = 3
state = "working"
url = "https://github.com/YunoHost-Apps/20euros_ynh"

[vieux-truc]
category = "games"
state = "notworking"
deprecated_date = 1700000000
url = "https://github.com/YunoHost-Apps/vieux-truc_ynh"
"#;

    #[test]
    fn le_niveau_et_l_etat_se_lisent() {
        let c = Catalogue::lire(TOML);
        assert_eq!(c.nombre(), 3);
        let p = c.trouver("13ft").unwrap();
        assert_eq!(p.niveau, Some(8));
        assert_eq!(p.etat, "working");
        assert!(p.solide());
    }

    #[test]
    fn un_paquet_se_retrouve_par_le_depot_du_logiciel_amont() {
        // Les deux catalogues ne nomment pas toujours pareil : `foo_ynh` sur
        // la forge correspond au logiciel `foo`.
        let c = Catalogue::lire(TOML);
        assert_eq!(
            c.trouver("https://github.com/YunoHost-Apps/13ft")
                .map(|p| p.id.as_str()),
            Some("13ft")
        );
    }

    #[test]
    fn un_paquet_peu_avance_n_est_pas_dit_solide() {
        // Le niveau 4 est le seuil : en deca, la mise a jour et la sauvegarde
        // ne sont pas verifiees.
        let c = Catalogue::lire(TOML);
        assert!(!c.trouver("20euros").unwrap().solide());
        assert_eq!(
            c.trouver("20euros").unwrap().reserves,
            vec!["non-free-assets"]
        );
    }

    #[test]
    fn un_paquet_sans_niveau_le_dit_plutot_que_d_inventer_un_zero() {
        let c = Catalogue::lire(TOML);
        let p = c.trouver("vieux-truc").unwrap();
        assert_eq!(p.niveau, None);
        assert!(p.ce_que_dit_le_niveau().contains("pas encore"));
        assert_eq!(p.retire_le, Some(1700000000));
        assert!(!p.solide());
    }

    #[test]
    fn chaque_niveau_se_dit_en_clair() {
        for n in 0..=8u8 {
            let p = Paquet {
                niveau: Some(n),
                ..Default::default()
            };
            assert!(!p.ce_que_dit_le_niveau().is_empty(), "niveau {n}");
        }
    }

    #[test]
    fn un_catalogue_illisible_ne_fait_pas_echouer_la_lecture() {
        assert_eq!(Catalogue::lire("pas du toml [[[").nombre(), 0);
        assert!(Catalogue::lire("").trouver("x").is_none());
    }
}
