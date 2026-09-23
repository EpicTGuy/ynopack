//! Reconnaissance d'une URL de depot.
//!
//! L'utilisateur colle ce qu'il a sous la main : une URL de navigateur, une
//! adresse de clonage, avec ou sans `.git`, avec ou sans branche. Tout doit
//! mener au meme depot.

use ynp_core::facts::{Forge, SourceRef};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UrlError {
    #[error("URL de depot non reconnue : {0}")]
    Unrecognized(String),
    #[error("forge non prise en charge pour le moment : {0}")]
    UnsupportedForge(String),
}

/// Analyse une URL de depot et en tire proprietaire, nom et forge.
pub fn parse(input: &str) -> Result<SourceRef, UrlError> {
    let raw = input.trim();
    let cleaned = raw
        .trim_end_matches('/')
        .trim_start_matches("git@")
        .replace("https://", "")
        .replace("http://", "")
        .replace("ssh://", "")
        // Forme SSH `git@github.com:owner/repo`
        .replacen(".com:", ".com/", 1)
        .replacen(".org:", ".org/", 1);

    let mut parts = cleaned.split('/').filter(|s| !s.is_empty());
    let host = parts
        .next()
        .ok_or_else(|| UrlError::Unrecognized(raw.into()))?
        .to_lowercase();
    let owner = parts
        .next()
        .ok_or_else(|| UrlError::Unrecognized(raw.into()))?
        .to_string();
    let repo = parts
        .next()
        .ok_or_else(|| UrlError::Unrecognized(raw.into()))?
        .trim_end_matches(".git")
        .to_string();

    if owner.is_empty() || repo.is_empty() {
        return Err(UrlError::Unrecognized(raw.into()));
    }

    let forge = match host.as_str() {
        "github.com" | "www.github.com" => Forge::GitHub,
        "gitlab.com" => Forge::GitLab,
        h if h.contains("codeberg") => Forge::Forgejo,
        h if h.contains("gitea") => Forge::Gitea,
        other => return Err(UrlError::UnsupportedForge(other.to_string())),
    };

    if forge != Forge::GitHub {
        // Les autres forges partagent l'API de Gitea/Forgejo : le support
        // viendra, mais annoncer un succes qu'on ne tient pas serait pire.
        return Err(UrlError::UnsupportedForge(host));
    }

    Ok(SourceRef {
        forge,
        url: format!("https://{host}/{owner}/{repo}"),
        owner,
        repo,
        default_branch: None,
        commit: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toutes_les_formes_d_url_menent_au_meme_depot() {
        let attendu = ("gristlabs", "grist-core");
        for url in [
            "https://github.com/gristlabs/grist-core",
            "https://github.com/gristlabs/grist-core/",
            "https://github.com/gristlabs/grist-core.git",
            "http://github.com/gristlabs/grist-core",
            "github.com/gristlabs/grist-core",
            "git@github.com:gristlabs/grist-core.git",
            "https://github.com/gristlabs/grist-core/tree/main",
        ] {
            let s = parse(url).unwrap_or_else(|e| panic!("{url} : {e}"));
            assert_eq!((s.owner.as_str(), s.repo.as_str()), attendu, "pour {url}");
            assert_eq!(s.url, "https://github.com/gristlabs/grist-core");
        }
    }

    #[test]
    fn une_forge_non_encore_prise_en_charge_est_annoncee_comme_telle() {
        // Mieux vaut un refus clair qu'un succes qu'on ne tient pas.
        assert!(matches!(
            parse("https://gitlab.com/owner/repo"),
            Err(UrlError::UnsupportedForge(_))
        ));
    }

    #[test]
    fn une_url_incomplete_est_refusee() {
        assert!(parse("https://github.com/gristlabs").is_err());
        assert!(parse("").is_err());
    }
}
