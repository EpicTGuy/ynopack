//! Recherche d'applications a packager, et de leurs alternatives.
//!
//! AlternativeTo aurait ete le reflexe, mais le site repond 403 a toute
//! requete automatisee et son API est reservee a ses partenaires. On s'appuie
//! donc sur `awesome-selfhosted-data`, qui est mieux adapte au besoin :
//! ses mille entrees sont auto-hebergeables par construction, chacune porte
//! son depot source, sa licence et ses categories, et le tout est du YAML
//! librement accessible.
//!
//! Les alternatives sont les logiciels partageant une categorie. Croisees avec
//! le catalogue YunoHost, elles disent ce qui reste a packager.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const ARCHIVE: &str =
    "https://github.com/awesome-selfhosted/awesome-selfhosted-data/archive/refs/heads/master.tar.gz";
const CATALOGUE: &str = "https://raw.githubusercontent.com/YunoHost/apps/main/apps.toml";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Logiciel {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub website_url: String,
    #[serde(default)]
    pub source_code_url: String,
    #[serde(default)]
    pub licenses: Vec<String>,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub stargazers_count: u32,
    #[serde(default)]
    pub updated_at: String,
    /// Renseigne apres croisement avec le catalogue YunoHost.
    #[serde(default, skip_deserializing)]
    pub deja_package: bool,
}

impl Logiciel {
    /// Vrai si `yunopack` sait analyser ce depot aujourd'hui.
    pub fn analysable(&self) -> bool {
        self.source_code_url.contains("github.com")
    }

    /// Vrai si la licence figure parmi celles que le catalogue accepte.
    ///
    /// Une licence inconnue ne disqualifie pas : elle demande une verification.
    pub fn licence_libre(&self) -> bool {
        const LIBRES: &[&str] = &[
            "AGPL-3.0",
            "GPL-3.0",
            "GPL-2.0",
            "LGPL-3.0",
            "LGPL-2.1",
            "MIT",
            "ISC",
            "Apache-2.0",
            "MPL-2.0",
            "BSD-2-Clause",
            "BSD-3-Clause",
            "Unlicense",
            "CC0-1.0",
            "EUPL-1.2",
            "Zlib",
        ];
        self.licenses
            .iter()
            .any(|l| LIBRES.iter().any(|b| l.starts_with(b)))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AlternativesError {
    #[error("recuperation du catalogue externe : {0}")]
    Http(#[from] reqwest::Error),
    #[error("archive du catalogue externe illisible : {0}")]
    Archive(#[from] std::io::Error),
}

/// Le catalogue externe, charge une fois puis interroge localement.
pub struct Catalogue {
    logiciels: Vec<Logiciel>,
}

impl Catalogue {
    /// Telecharge le catalogue et marque ce qui est deja package.
    pub async fn charger() -> Result<Self, AlternativesError> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("yunopack/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let octets = client.get(ARCHIVE).send().await?.bytes().await?;
        let mut logiciels = extraire(&octets)?;

        let packagees = deja_packagees(&client).await.unwrap_or_default();
        for l in &mut logiciels {
            l.deja_package = packagees.contains(&depot_normalise(&l.source_code_url))
                || packagees.contains(&l.name.to_lowercase());
        }

        // Les plus suivis d'abord : c'est le meilleur indice de ce qui
        // interesse le plus de monde.
        logiciels.sort_by_key(|l| std::cmp::Reverse(l.stargazers_count));
        Ok(Self { logiciels })
    }

    pub fn nombre(&self) -> usize {
        self.logiciels.len()
    }

    /// Logiciels dont le nom ou la description contient le terme.
    pub fn chercher(&self, terme: &str) -> Vec<&Logiciel> {
        let t = terme.to_lowercase();
        self.logiciels
            .iter()
            .filter(|l| {
                l.name.to_lowercase().contains(&t) || l.description.to_lowercase().contains(&t)
            })
            .collect()
    }

    /// Alternatives a un logiciel : ceux qui partagent au moins une categorie.
    ///
    /// Le logiciel de depart est exclu, et le tri privilegie ceux qui partagent
    /// le plus de categories — donc les plus proches — puis les plus suivis.
    pub fn alternatives(&self, nom: &str) -> Vec<&Logiciel> {
        let t = nom.to_lowercase();
        let Some(reference) = self
            .logiciels
            .iter()
            .find(|l| l.name.to_lowercase() == t)
            .or_else(|| {
                self.logiciels
                    .iter()
                    .find(|l| l.name.to_lowercase().contains(&t))
            })
        else {
            return Vec::new();
        };

        let categories: BTreeSet<&String> = reference.tags.iter().collect();
        let mut trouves: Vec<(usize, &Logiciel)> = self
            .logiciels
            .iter()
            .filter(|l| l.name != reference.name)
            .filter_map(|l| {
                let communes = l.tags.iter().filter(|t| categories.contains(t)).count();
                (communes > 0).then_some((communes, l))
            })
            .collect();

        trouves.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then(b.1.stargazers_count.cmp(&a.1.stargazers_count))
        });
        trouves.into_iter().map(|(_, l)| l).collect()
    }

    /// Ce qui reste a packager : ni au catalogue, ni sur une forge inconnue.
    pub fn a_packager<'a>(&'a self, parmi: &[&'a Logiciel]) -> Vec<&'a Logiciel> {
        parmi
            .iter()
            .copied()
            .filter(|l| !l.deja_package && l.analysable())
            .collect()
    }
}

/// Lit les fichiers `software/*.yml` de l'archive.
fn extraire(gzip: &[u8]) -> Result<Vec<Logiciel>, std::io::Error> {
    use std::io::Read;

    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(gzip));
    let mut out = Vec::new();

    for entree in archive.entries()? {
        let mut entree = entree?;
        let chemin = entree.path()?.to_string_lossy().into_owned();
        if !chemin.contains("/software/") || !chemin.ends_with(".yml") {
            continue;
        }
        let mut contenu = String::new();
        if entree.read_to_string(&mut contenu).is_err() {
            continue;
        }
        // Une entree mal formee ne doit pas faire perdre les neuf cents autres.
        if let Ok(l) = serde_yaml::from_str::<Logiciel>(&contenu) {
            if !l.name.is_empty() {
                out.push(l);
            }
        }
    }
    Ok(out)
}

async fn deja_packagees(client: &reqwest::Client) -> Result<BTreeSet<String>, AlternativesError> {
    let brut = client.get(CATALOGUE).send().await?.text().await?;
    let table: BTreeMap<String, toml::Value> = toml::from_str(&brut).unwrap_or_default();

    let mut out = BTreeSet::new();
    for (id, entree) in table {
        out.insert(id.to_lowercase());
        if let Some(url) = entree.get("url").and_then(|u| u.as_str()) {
            out.insert(depot_normalise(url.trim_end_matches("_ynh")));
        }
    }
    Ok(out)
}

/// `owner/repo` en minuscules, pour comparer deux URL de forge.
fn depot_normalise(url: &str) -> String {
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

    fn logiciel(nom: &str, tags: &[&str], etoiles: u32) -> Logiciel {
        Logiciel {
            name: nom.into(),
            source_code_url: format!("https://github.com/x/{}", nom.to_lowercase()),
            licenses: vec!["MIT".into()],
            tags: tags.iter().map(|t| t.to_string()).collect(),
            stargazers_count: etoiles,
            ..Default::default()
        }
    }

    fn catalogue() -> Catalogue {
        Catalogue {
            logiciels: vec![
                logiciel("Miniflux", &["Feed Readers"], 9000),
                logiciel("FreshRSS", &["Feed Readers"], 8000),
                logiciel("Tiny Tiny RSS", &["Feed Readers", "News"], 3000),
                logiciel("Nextcloud", &["File Sharing"], 27000),
            ],
        }
    }

    #[test]
    fn les_alternatives_partagent_une_categorie_et_excluent_le_point_de_depart() {
        let c = catalogue();
        let noms: Vec<&str> = c
            .alternatives("Miniflux")
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(noms, vec!["FreshRSS", "Tiny Tiny RSS"]);
    }

    #[test]
    fn celles_qui_partagent_le_plus_de_categories_viennent_en_premier() {
        let mut c = catalogue();
        c.logiciels
            .push(logiciel("Proche", &["Feed Readers", "News"], 10));
        // « Tiny Tiny RSS » partage aussi deux categories mais a plus d'etoiles.
        let alt = c.alternatives("Tiny Tiny RSS");
        assert_eq!(alt[0].name, "Proche");
    }

    #[test]
    fn un_nom_partiel_suffit_a_retrouver_le_logiciel() {
        let c = catalogue();
        assert!(!c.alternatives("minifl").is_empty());
    }

    #[test]
    fn un_nom_inconnu_rend_une_liste_vide_plutot_qu_une_erreur() {
        let c = catalogue();
        assert!(c.alternatives("logiciel-inexistant").is_empty());
    }

    #[test]
    fn la_recherche_porte_aussi_sur_la_description() {
        let mut c = catalogue();
        c.logiciels[0].description = "Lecteur de flux minimaliste".into();
        assert_eq!(c.chercher("minimaliste").len(), 1);
    }

    #[test]
    fn ne_restent_a_packager_que_les_depots_analysables_et_absents_du_catalogue() {
        let mut c = catalogue();
        c.logiciels[0].deja_package = true; // Miniflux est deja au catalogue
        c.logiciels[1].source_code_url = "https://gitlab.com/x/freshrss".into();

        let tous: Vec<&Logiciel> = c.logiciels.iter().collect();
        let restants: Vec<&str> = c
            .a_packager(&tous)
            .iter()
            .map(|l| l.name.as_str())
            .collect();

        assert!(!restants.contains(&"Miniflux"), "deja package");
        assert!(!restants.contains(&"FreshRSS"), "forge non prise en charge");
        assert!(restants.contains(&"Nextcloud"));
    }

    #[test]
    fn une_licence_inconnue_ne_disqualifie_pas_mais_se_distingue() {
        let mut l = logiciel("X", &[], 0);
        assert!(l.licence_libre());
        l.licenses = vec!["BSL-1.1".into()];
        assert!(!l.licence_libre());
    }

    #[test]
    fn deux_urls_du_meme_depot_se_comparent_egales() {
        assert_eq!(depot_normalise("https://github.com/Foo/Bar/"), "foo/bar");
        assert_eq!(depot_normalise("https://github.com/foo/bar.git"), "foo/bar");
    }

    #[test]
    fn le_format_reel_d_une_fiche_se_relit() {
        // Copie telle quelle de software/miniflux.yml.
        let y = r#"
name: Miniflux
website_url: https://miniflux.app/
description: Minimalist news reader.
licenses:
  - Apache-2.0
platforms:
  - Go
  - deb
tags:
  - Feed Readers
source_code_url: https://github.com/miniflux/v2
stargazers_count: 9722
updated_at: '2026-09-22'
"#;
        let l: Logiciel = serde_yaml::from_str(y).unwrap();
        assert_eq!(l.name, "Miniflux");
        assert_eq!(l.tags, vec!["Feed Readers"]);
        assert_eq!(l.stargazers_count, 9722);
        assert!(l.analysable());
        assert!(l.licence_libre());
    }
}
