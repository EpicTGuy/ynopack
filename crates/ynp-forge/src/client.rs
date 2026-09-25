//! Le client de la forge qui heberge un depot donne.
//!
//! Deux implementations pour l'instant : GitHub, et Gitea — dont Forgejo est
//! une fourche, Codeberg une instance. Les masquer derriere un trait commun
//! n'apporterait rien ici : les appels sont quatre, et une enumeration se lit
//! mieux qu'une indirection.
//!
//! Une instance auto-hebergee ne se reconnait pas a son nom de domaine :
//! `git.exemple.fr` peut faire tourner n'importe quoi. On l'interroge donc,
//! plutot que de deviner — c'est ce que fait l'outil partout ailleurs.

use crate::gitea::Gitea;
use crate::github::{ForgeError, GitHub};
use ynp_core::facts::{Forge, Release, RepoMeta, SourceRef};

pub enum Client {
    GitHub(GitHub),
    Gitea(Gitea),
}

impl Client {
    /// Choisit le client d'apres la forge reconnue, en interrogeant l'instance
    /// quand le nom de domaine ne dit rien.
    pub async fn pour(source: &SourceRef) -> Result<Self, ForgeError> {
        let hote = hote_de(&source.url);
        match source.forge {
            Forge::GitHub => Ok(Client::GitHub(GitHub::new()?)),
            Forge::Gitea | Forge::Forgejo => Ok(Client::Gitea(Gitea::new(&hote)?)),
            Forge::GitLab => Err(ForgeError::Unexpected {
                status: 0,
                body: format!("{hote} : l'API GitLab n'est pas encore prise en charge"),
            }),
            Forge::Inconnue => {
                if est_une_instance_gitea(&hote).await {
                    Ok(Client::Gitea(Gitea::new(&hote)?))
                } else {
                    Err(ForgeError::Unexpected {
                        status: 0,
                        body: format!(
                            "{hote} : forge non reconnue — GitHub, Gitea et Forgejo \
                             (dont Codeberg) sont pris en charge"
                        ),
                    })
                }
            }
        }
    }

    pub async fn repo(&self, source: &mut SourceRef) -> Result<RepoMeta, ForgeError> {
        match self {
            Client::GitHub(c) => c.repo(source).await,
            Client::Gitea(c) => c.repo(source).await,
        }
    }

    pub async fn releases(&self, source: &SourceRef) -> Result<Vec<Release>, ForgeError> {
        match self {
            Client::GitHub(c) => c.releases(source).await,
            Client::Gitea(c) => c.releases(source).await,
        }
    }

    pub async fn tags(&self, source: &SourceRef) -> Result<Vec<String>, ForgeError> {
        match self {
            Client::GitHub(c) => c.tags(source).await,
            Client::Gitea(c) => c.tags(source).await,
        }
    }

    pub async fn download(&self, url: &str) -> Result<Vec<u8>, ForgeError> {
        match self {
            Client::GitHub(c) => c.download(url).await,
            Client::Gitea(c) => c.download(url).await,
        }
    }

    pub fn archive_url(&self, source: &SourceRef, reference: &str) -> String {
        match self {
            Client::GitHub(_) => crate::github::archive_url(source, reference),
            Client::Gitea(c) => crate::gitea::archive_url(c.hote(), source, reference),
        }
    }
}

/// L'hote d'une URL, sans schema ni chemin.
pub fn hote_de(url: &str) -> String {
    url.trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

/// Vrai si l'instance repond a l'API de Gitea.
///
/// `/api/v1/version` est le point le plus leger qu'exposent Gitea et Forgejo,
/// et il ne demande aucune authentification. Une reponse est un fait ; son
/// absence n'accuse personne, elle dit seulement qu'on ne sait pas.
async fn est_une_instance_gitea(hote: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .user_agent(concat!("yunopack/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(8))
        .build()
    else {
        return false;
    };
    let Ok(r) = client
        .get(format!("https://{hote}/api/v1/version"))
        .send()
        .await
    else {
        return false;
    };
    if !r.status().is_success() {
        return false;
    }
    r.text()
        .await
        .is_ok_and(|t| t.contains("\"version\"") && t.len() < 512)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_hote_se_tire_de_n_importe_quelle_forme_d_url() {
        for u in [
            "https://codeberg.org/a/b",
            "http://codeberg.org/a/b",
            "codeberg.org/a/b",
            "https://CODEBERG.org/a/b/tree/main",
        ] {
            assert_eq!(hote_de(u), "codeberg.org", "{u}");
        }
    }

    #[tokio::test]
    async fn gitlab_est_refuse_avec_une_raison_lisible() {
        let s = SourceRef {
            forge: Forge::GitLab,
            owner: "a".into(),
            repo: "b".into(),
            url: "https://gitlab.com/a/b".into(),
            default_branch: None,
            commit: None,
        };
        let e = Client::pour(&s).await.err().expect("doit refuser");
        assert!(e.to_string().contains("GitLab"), "{e}");
    }
}
