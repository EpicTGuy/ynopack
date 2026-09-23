//! Acces aux forges : metadonnees, releases, archive du depot.
//!
//! Seule partie du projet qui parle au reseau, et elle ne parle qu'a des
//! forges. C'est ce qui permet a la garantie de l'ADR-002 — aucun appel a un
//! modele de langage — d'etre verifiable en lisant un seul crate.

pub mod alternatives;
pub mod archive;
pub mod github;
pub mod prebuilt;
pub mod sources;
pub mod url;
pub mod wishlist;

use ynp_analyze::ForgeData;
use ynp_core::tree::RepoTree;

pub use github::{ForgeError, GitHub};
pub use sources::{SourceChoice, SourceKind};
pub use url::UrlError;
pub use wishlist::Souhait;

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
    /// Source retenue, avec les binaires preconstruits s'il y en a.
    pub selection: ynp_core::facts::SourceSelection,
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

    // Les binaires deja construits, quand l'amont en publie, evitent de
    // compiler sur la machine cible — ce que font tous les paquets YunoHost de
    // reference. Leur somme de controle demande un telechargement chacun ;
    // c'est le prix d'un paquet qui s'installe sur une petite instance.
    let mut prebuilt = releases
        .iter()
        .find(|r| r.tag == choice.reference)
        .map(prebuilt::select)
        .unwrap_or_default();

    for asset in &mut prebuilt {
        let bytes = client.download(&asset.url).await?;
        asset.sha256 = sources::sha256(&bytes);
    }

    let selection = ynp_core::facts::SourceSelection {
        reference: choice.reference.clone(),
        strategy: choice.strategy.clone(),
        version: choice.version.clone(),
        kind: match choice.kind {
            SourceKind::Release => "release",
            SourceKind::Tag => "tag",
            SourceKind::Commit => "commit",
        }
        .to_string(),
        url: choice.url.clone(),
        sha256: sha256.clone(),
        prebuilt,
    };

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
        selection,
    })
}
