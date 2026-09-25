//! Liste de souhaits de YunoHost.
//!
//! Le projet maintient dans son depot de catalogue une liste d'applications
//! que la communaute aimerait voir packagees. C'est exactement la matiere
//! premiere de cet outil : cinq cents demandes deja triees, avec leur depot
//! amont.
//!
//! Plutot que de chercher quoi packager, on part de ce que des gens ont
//! demande.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const URL: &str = "https://raw.githubusercontent.com/YunoHost/apps/main/wishlist.toml";
const URL_CATALOGUE: &str = "https://raw.githubusercontent.com/YunoHost/apps/main/apps.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Souhait {
    /// Identifiant dans la liste, qui deviendra souvent l'identifiant de l'app.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Depot amont : c'est lui qu'on donnera a `yunopack`.
    pub upstream: String,
    #[serde(default)]
    pub website: String,
    /// Horodatage d'ajout a la liste.
    #[serde(default)]
    pub added_date: i64,
    /// URL d'un paquet deja en preparation, quand quelqu'un s'en occupe.
    ///
    /// Le champ s'appelle `draft` dans la liste amont, mais il ne vaut jamais
    /// un booleen : il porte l'adresse du depot ou le travail a commence.
    /// S'y attaquer ferait donc doublon.
    #[serde(default)]
    pub draft: Option<String>,
}

impl Souhait {
    /// Forge hebergeant le depot amont, telle qu'on peut la reconnaitre.
    pub fn forge(&self) -> &str {
        for (motif, nom) in [
            ("github.com", "github"),
            ("gitlab.com", "gitlab"),
            ("codeberg.org", "codeberg"),
            ("framagit.org", "gitlab"),
            ("salsa.debian.org", "gitlab"),
            ("0xacab.org", "gitlab"),
            // Sourcehut s'ecrit aussi bien `sr.ht` que `git.sr.ht` selon les
            // fiches ; ne reconnaitre que la seconde en laissait passer trois.
            ("sr.ht", "sourcehut"),
            ("bitbucket.org", "bitbucket"),
        ] {
            if self.upstream.contains(motif) {
                return nom;
            }
        }
        "autre"
    }

    /// Vrai si `yunopack` sait analyser cette forge aujourd'hui.
    ///
    /// Codeberg fait tourner Forgejo, dont l'API est celle de Gitea : un seul
    /// client sert les deux. Les instances auto-hebergees ne se reconnaissent
    /// pas a leur domaine ; c'est `fetch` qui les interroge, et on les tente
    /// plutot que de les ecarter d'avance.
    pub fn analysable(&self) -> bool {
        !matches!(self.forge(), "gitlab" | "sourcehut" | "bitbucket")
            && self.upstream.starts_with("http")
    }

    /// Vrai si quelqu'un a deja commence le paquet.
    pub fn en_cours(&self) -> bool {
        self.draft.is_some()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WishlistError {
    #[error("recuperation de la liste de souhaits : {0}")]
    Http(#[from] reqwest::Error),
    #[error("la liste de souhaits n'est pas un TOML valide : {0}")]
    Format(#[from] toml::de::Error),
}

/// Recupere la liste, et retire ce qui est deja package.
///
/// Proposer de packager une application qui existe deja au catalogue ferait
/// perdre du temps : le rapprochement se fait sur l'identifiant et sur l'URL
/// du depot amont, car les deux listes ne nomment pas toujours pareil.
pub async fn recuperer() -> Result<Vec<Souhait>, WishlistError> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("yunopack/", env!("CARGO_PKG_VERSION")))
        .build()?;

    let brut = client.get(URL).send().await?.text().await?;
    let table: BTreeMap<String, SouhaitBrut> = toml::from_str(&brut)?;

    let deja = deja_packagees(&client).await.unwrap_or_default();

    Ok(table
        .into_iter()
        .map(|(id, s)| Souhait {
            id,
            name: s.name,
            description: s.description,
            upstream: s.upstream,
            website: s.website,
            added_date: s.added_date,
            draft: s.draft,
        })
        .filter(|s| !deja.contains(&s.id) && !deja.contains(&normaliser(&s.upstream)))
        .collect())
}

/// Identifiants et depots des applications deja au catalogue.
async fn deja_packagees(client: &reqwest::Client) -> Result<Vec<String>, WishlistError> {
    let brut = client.get(URL_CATALOGUE).send().await?.text().await?;
    let table: BTreeMap<String, toml::Value> = toml::from_str(&brut)?;

    let mut out = Vec::new();
    for (id, entree) in table {
        out.push(id);
        if let Some(url) = entree.get("url").and_then(|u| u.as_str()) {
            // `foo_ynh` sur la forge correspond a l'app `foo`.
            out.push(normaliser(url.trim_end_matches("_ynh")));
        }
    }
    Ok(out)
}

/// Forme comparable d'une URL de depot : `owner/repo` en minuscules.
fn normaliser(url: &str) -> String {
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("/")
        .to_lowercase()
}

#[derive(Debug, Deserialize)]
struct SouhaitBrut {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    upstream: String,
    #[serde(default)]
    website: String,
    #[serde(default)]
    added_date: i64,
    #[serde(default)]
    draft: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn souhait(upstream: &str) -> Souhait {
        Souhait {
            id: "demo".into(),
            name: "Demo".into(),
            description: String::new(),
            upstream: upstream.into(),
            website: String::new(),
            added_date: 0,
            draft: None,
        }
    }

    #[test]
    fn la_forge_est_reconnue_a_son_domaine() {
        assert_eq!(souhait("https://github.com/x/y").forge(), "github");
        assert_eq!(souhait("https://codeberg.org/x/y").forge(), "codeberg");
        assert_eq!(
            souhait("https://git.lolcat.ca/lolcat/4get").forge(),
            "autre"
        );
    }

    #[test]
    fn seules_les_forges_prises_en_charge_sont_analysables() {
        // Annoncer comme analysable un depot qu'on ne sait pas lire ferait
        // perdre du temps a qui clique dessus.
        assert!(souhait("https://github.com/x/y").analysable());
        assert!(!souhait("https://gitlab.com/x/y").analysable());
    }

    #[test]
    fn les_urls_se_comparent_sur_proprietaire_et_depot() {
        assert_eq!(
            normaliser("https://github.com/YunoHost-Apps/miniflux_ynh"),
            "yunohost-apps/miniflux_ynh"
        );
        assert_eq!(normaliser("https://github.com/Foo/Bar/"), "foo/bar");
        assert_eq!(normaliser("https://github.com/Foo/Bar.git"), "foo/bar");
    }

    #[test]
    fn le_format_reel_de_la_liste_se_relit() {
        // Extrait copie tel quel du wishlist.toml officiel.
        let brut = r#"
[3xui]
name = "3XUI"
description = "VPN panel for Xray servers"
upstream = "https://github.com/MHSanaei/3x-ui"
website = ""
added_date = 1772904823

[en-cours]
name = "En cours"
description = "Quelqu'un s'en occupe deja"
upstream = "https://github.com/x/y"
draft = "https://github.com/YunoHost-Apps/beatbump_ynh"
"#;
        let table: BTreeMap<String, SouhaitBrut> = toml::from_str(brut).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table["3xui"].name, "3XUI");
        // `draft` ne vaut jamais un booleen dans la liste reelle : il porte
        // l'URL du paquet deja en preparation. L'avoir suppose booleen faisait
        // echouer la lecture de toute la liste.
        assert!(table["en-cours"].draft.is_some());
        assert!(table["3xui"].draft.is_none());
        // `website` et `added_date` manquent sur la seconde : le defaut evite
        // de rejeter la moitie de la liste.
        assert_eq!(table["en-cours"].website, "");
    }
}
