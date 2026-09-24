//! Catalogue consommable par `yunohost tools update apps`.
//!
//! Le format est celui que servent les instances YunoHost sous
//! `/v3/apps.json`, releve sur le cache d'une instance reelle plutot que
//! devine : `from_api_version`, `apps`, `categories`, `antifeatures`.
//!
//! Un catalogue maison se declare dans `/etc/yunohost/apps_catalog.yml` et
//! cohabite avec le catalogue officiel.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Version d'API du catalogue attendue par YunoHost 12.
const API_VERSION: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalogue {
    pub apps: Map<String, Value>,
    pub categories: Vec<Value>,
    pub antifeatures: Vec<Value>,
    pub from_api_version: u32,
}

impl Default for Catalogue {
    fn default() -> Self {
        Self {
            apps: Map::new(),
            categories: Vec::new(),
            antifeatures: Vec::new(),
            from_api_version: API_VERSION,
        }
    }
}

impl Catalogue {
    /// Relit un catalogue existant, ou en cree un vide.
    ///
    /// Une lecture qui echoue ne doit pas effacer le catalogue : on le
    /// signale, mais on ne repart pas de zero en silence.
    pub fn charger(chemin: &std::path::Path) -> Result<Self, CatalogueError> {
        match std::fs::read_to_string(chemin) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(CatalogueError::Lecture {
                chemin: chemin.display().to_string(),
                source,
            }),
            Ok(texte) => serde_json::from_str(&texte).map_err(|source| CatalogueError::Format {
                chemin: chemin.display().to_string(),
                source,
            }),
        }
    }

    /// Ajoute ou remplace une application.
    ///
    /// `level = 0` tant que `package_check` n'a pas tourne : annoncer un
    /// niveau qu'on n'a pas mesure tromperait celui qui lit le catalogue.
    pub fn inscrire(
        &mut self,
        id: &str,
        manifest: Value,
        url_git: &str,
        branche: &str,
        revision: &str,
        niveau: Option<u8>,
    ) {
        let maintenant = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        self.apps.insert(
            id.to_string(),
            json!({
                "id": id,
                "git": { "url": url_git, "branch": branche, "revision": revision },
                "lastUpdate": maintenant,
                "manifest": manifest,
                "state": "working",
                "level": niveau.unwrap_or(0),
                "maintained": true,
                "high_quality": false,
                "featured": false,
                "category": Value::Null,
                "subtags": json!([]),
                "antifeatures": json!([]),
                "potential_alternative_to": json!([]),
            }),
        );
    }

    pub fn retirer(&mut self, id: &str) -> bool {
        self.apps.remove(id).is_some()
    }

    pub fn ecrire(&self, chemin: &std::path::Path) -> Result<(), CatalogueError> {
        if let Some(parent) = chemin.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CatalogueError::Ecriture {
                chemin: parent.display().to_string(),
                source,
            })?;
        }
        let texte = serde_json::to_string_pretty(self).expect("catalogue serialisable");
        std::fs::write(chemin, texte).map_err(|source| CatalogueError::Ecriture {
            chemin: chemin.display().to_string(),
            source,
        })
    }

    pub fn nombre_d_apps(&self) -> usize {
        self.apps.len()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogueError {
    #[error("lecture de {chemin} : {source}")]
    Lecture {
        chemin: String,
        source: std::io::Error,
    },
    #[error("{chemin} n'est pas un catalogue valide : {source}")]
    Format {
        chemin: String,
        source: serde_json::Error,
    },
    #[error("ecriture de {chemin} : {source}")]
    Ecriture {
        chemin: String,
        source: std::io::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Value {
        json!({ "packaging_format": 2, "id": "demo", "version": "1.0.0~ynh1" })
    }

    #[test]
    fn un_catalogue_neuf_annonce_la_bonne_version_d_api() {
        let c = Catalogue::default();
        assert_eq!(c.from_api_version, 3);
        assert_eq!(c.nombre_d_apps(), 0);
    }

    #[test]
    fn une_app_inscrite_porte_tout_ce_que_yunohost_attend() {
        let mut c = Catalogue::default();
        c.inscrire(
            "demo",
            manifest(),
            "https://git.x/demo_ynh",
            "main",
            "abc123",
            None,
        );

        let a = &c.apps["demo"];
        assert_eq!(a["id"], "demo");
        assert_eq!(a["state"], "working");
        assert_eq!(a["git"]["url"], "https://git.x/demo_ynh");
        assert_eq!(a["git"]["revision"], "abc123");
        assert_eq!(a["manifest"]["packaging_format"], 2);
        assert!(a["lastUpdate"].as_u64().unwrap() > 1_700_000_000);
    }

    #[test]
    fn le_niveau_reste_a_zero_tant_qu_il_n_a_pas_ete_mesure() {
        // Annoncer un niveau non mesure tromperait celui qui lit le catalogue.
        let mut c = Catalogue::default();
        c.inscrire("demo", manifest(), "u", "main", "r", None);
        assert_eq!(c.apps["demo"]["level"], 0);

        c.inscrire("demo", manifest(), "u", "main", "r", Some(6));
        assert_eq!(c.apps["demo"]["level"], 6);
    }

    #[test]
    fn reinscrire_une_app_la_remplace_sans_la_dupliquer() {
        let mut c = Catalogue::default();
        c.inscrire("demo", manifest(), "u", "main", "v1", None);
        c.inscrire("demo", manifest(), "u", "main", "v2", None);

        assert_eq!(c.nombre_d_apps(), 1);
        assert_eq!(c.apps["demo"]["git"]["revision"], "v2");
    }

    #[test]
    fn un_catalogue_absent_est_cree_plutot_que_rate() {
        let chemin = std::env::temp_dir().join("yunopack-catalogue-absent.json");
        let _ = std::fs::remove_file(&chemin);
        assert_eq!(Catalogue::charger(&chemin).unwrap().nombre_d_apps(), 0);
    }

    #[test]
    fn un_catalogue_illisible_est_signale_et_non_ecrase() {
        // Repartir de zero en silence ferait disparaitre toutes les apps deja
        // publiees.
        let chemin = std::env::temp_dir().join("yunopack-catalogue-casse.json");
        std::fs::write(&chemin, "{ ceci n'est pas du json").unwrap();
        assert!(matches!(
            Catalogue::charger(&chemin),
            Err(CatalogueError::Format { .. })
        ));
    }

    #[test]
    fn un_catalogue_fait_un_aller_retour_sur_disque() {
        let chemin = std::env::temp_dir().join("yunopack-catalogue-ar.json");
        let mut c = Catalogue::default();
        c.inscrire("demo", manifest(), "u", "main", "r", Some(4));
        c.ecrire(&chemin).unwrap();

        let relu = Catalogue::charger(&chemin).unwrap();
        assert_eq!(relu.nombre_d_apps(), 1);
        assert_eq!(relu.apps["demo"]["level"], 4);
        assert_eq!(relu.from_api_version, 3);
    }
}
