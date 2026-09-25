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
/// Page ou la communaute vote. Les votes n'existent nulle part ailleurs :
/// `wishlist.toml` ne les porte pas, et le magasin n'a pas d'API qui les rende.
const URL_VOTES: &str = "https://apps.yunohost.org/wishlist";

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
    /// Votes de la communaute. C'est la seule mesure de la demande reelle.
    #[serde(default)]
    pub votes: u32,
    /// L'application est deja au catalogue : la demande peut etre retiree.
    #[serde(default)]
    pub deja_package: bool,
    #[serde(default)]
    pub id_yunohost: String,
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
    let votes = votes(&client).await.unwrap_or_default();

    let mut out: Vec<Souhait> = table
        .into_iter()
        .map(|(id, s)| {
            let par_depot = deja.get(&normaliser(&s.upstream));
            let par_id = deja.get(&id);
            let trouve = par_depot.or(par_id);
            Souhait {
                votes: votes.get(&id).copied().unwrap_or(0),
                deja_package: trouve.is_some(),
                id_yunohost: trouve.cloned().unwrap_or_default(),
                id,
                name: s.name,
                description: s.description,
                upstream: s.upstream,
                website: s.website,
                added_date: s.added_date,
                draft: s.draft,
            }
        })
        .collect();

    // Les plus demandees d'abord. L'ordre alphabetique de `wishlist.toml` ne
    // dit rien de ce que les gens attendent vraiment.
    out.sort_by(|a, b| {
        b.votes
            .cmp(&a.votes)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Votes par identifiant, lus sur la page publique.
///
/// Aucune API ne les rend : ni `wishlist.toml`, ni le magasin. La page reste
/// la seule source, et la lire est fragile — une refonte du gabarit y mettrait
/// fin. C'est pourquoi un echec n'en est pas un : sans votes, la liste reste
/// utilisable, simplement moins bien triee.
async fn votes(client: &reqwest::Client) -> Result<BTreeMap<String, u32>, WishlistError> {
    let html = client.get(URL_VOTES).send().await?.text().await?;
    Ok(extraire_votes(&html))
}

/// Chaque entree porte un lien `/app/<id>/star` suivi du compte.
fn extraire_votes(html: &str) -> BTreeMap<String, u32> {
    let mut out = BTreeMap::new();
    for bloc in html.split("href=\"/app/").skip(1) {
        let Some((id, reste)) = bloc.split_once("/star\"") else {
            continue;
        };
        if id.is_empty() || id.contains('"') || id.contains('<') {
            continue;
        }
        // Le premier nombre qui suit est le compte affiche ; le second est
        // celui qu'afficherait un clic, et ne nous interesse pas.
        let Some(apres) = reste
            .split_once("<span")
            .and_then(|(_, r)| r.split_once('>'))
        else {
            continue;
        };
        let nombre: String = apres
            .1
            .trim()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = nombre.parse::<u32>() {
            out.insert(id.to_string(), n);
        }
    }
    out
}

/// Cle de rapprochement -> identifiant de l'application au catalogue.
async fn deja_packagees(
    client: &reqwest::Client,
) -> Result<BTreeMap<String, String>, WishlistError> {
    let brut = client.get(URL_CATALOGUE).send().await?.text().await?;
    let table: BTreeMap<String, toml::Value> = toml::from_str(&brut)?;

    let mut out = BTreeMap::new();
    for (id, entree) in table {
        out.insert(id.clone(), id.clone());
        if let Some(url) = entree.get("url").and_then(|u| u.as_str()) {
            // `foo_ynh` sur la forge correspond a l'app `foo`.
            out.insert(normaliser(url.trim_end_matches("_ynh")), id.clone());
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
mod votes_et_perimes {
    use super::*;

    /// Extrait reel de la page, reduit a une entree.
    const PAGE: &str = r#"
      <td class="text-center">
        <a role="button" title="Star this app" href="/app/bigbluebutton/star" class="btn-sm">
            <span class="inline-block ">94</span>
            <span class="hidden ">95</span>
            <i class="fa fa-star-o inline-block "></i>
        </a>
      </td>
      <td>
        <a role="button" href="/app/anubis/star" class="btn-sm">
            <span class="inline-block ">51</span>
            <span class="hidden ">52</span>
        </a>
      </td>
    "#;

    #[test]
    fn les_votes_se_lisent_sur_la_page_publique() {
        // Aucune API ne les rend : ni wishlist.toml, ni le magasin.
        let v = extraire_votes(PAGE);
        assert_eq!(v.get("bigbluebutton"), Some(&94));
        assert_eq!(v.get("anubis"), Some(&51));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn c_est_le_compte_affiche_qui_est_retenu_pas_celui_d_apres_le_clic() {
        // Le second nombre est ce qu'afficherait un vote de plus.
        assert_eq!(extraire_votes(PAGE).get("bigbluebutton"), Some(&94));
    }

    #[test]
    fn une_page_refondue_ne_fait_pas_echouer_la_liste() {
        // La lecture d'un gabarit HTML est fragile par nature : sans votes, la
        // liste reste utilisable, simplement moins bien triee.
        assert!(extraire_votes("<html><body>rien ici</body></html>").is_empty());
        assert!(extraire_votes("").is_empty());
    }

    #[test]
    fn un_lien_malforme_est_ignore_sans_paniquer() {
        assert!(extraire_votes(r#"href="/app//star"><span >3</span>"#).is_empty());
        assert!(extraire_votes(r#"href="/app/x/star">pas de nombre"#).is_empty());
    }
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
            votes: 0,
            deja_package: false,
            id_yunohost: String::new(),
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
