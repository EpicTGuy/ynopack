//! Acces aux forges : metadonnees, releases, archive du depot.
//!
//! Seule partie du projet qui parle au reseau, et elle ne parle qu'a des
//! forges. C'est ce qui permet a la garantie de l'ADR-002 — aucun appel a un
//! modele de langage — d'etre verifiable en lisant un seul crate.

pub mod archive;
pub mod github;
pub mod sources;
pub mod url;

use ynp_analyze::ForgeData;
use ynp_core::tree::RepoTree;

pub use github::{ForgeError, GitHub};
pub use sources::{SourceChoice, SourceKind};
pub use url::UrlError;

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error(transparent)]
    Url(#[from] UrlError),
    #[error(transparent)]
    Forge(#[from] ForgeError),
}

/// Tout ce que la forge sait d'un depot, plus son contenu.
pub struct Fetched {
    pub forge: ForgeData,
    pub tree: RepoTree,
    pub choice: SourceChoice,
    /// Somme de controle de l'archive choisie, telle qu'elle ira au manifest.
    pub sha256: String,
}

/// Recupere un depot a partir de son URL.
///
/// L'archive est telechargee une seule fois : elle sert a la fois a l'analyse
/// du contenu et au calcul de la somme de controle qui figurera dans le
/// manifest. Telecharger deux fois exposerait a ce que l'amont change entre
/// les deux, et donc a publier une somme qui ne correspond a rien.
pub async fn fetch(repo_url: &str) -> Result<Fetched, FetchError> {
    let mut source = url::parse(repo_url)?;
    let client = GitHub::new()?;

    let meta = client.repo(&mut source).await?;
    let releases = client.releases(&source).await?;
    let tags = client.tags(&source).await?;

    let choice = sources::choose(&source, &releases, &tags);
    let bytes = client.download(&choice.url).await?;
    let sha256 = sources::sha256(&bytes);
    let tree = archive::extract(&bytes).map_err(ForgeError::from)?;

    source.commit = Some(choice.reference.clone());

    Ok(Fetched {
        forge: ForgeData {
            source,
            meta,
            releases,
            tags,
        },
        tree,
        choice,
        sha256,
    })
}
