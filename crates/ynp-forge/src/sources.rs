//! Choix de la source a telecharger, et calcul de sa somme de controle.
//!
//! Le manifest exige un `sha256` : c'est a la fois un controle d'integrite et
//! une protection contre une archive amont modifiee apres coup. Le choix de la
//! reference determine aussi la strategie de mise a jour automatique que
//! l'infrastructure YunoHost appliquera ensuite.

use sha2::{Digest, Sha256};
use ynp_core::facts::{Forge, Release, SourceRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceChoice {
    /// Tag, branche ou commit a archiver.
    pub reference: String,
    pub url: String,
    /// Valeur de `autoupdate.strategy` dans le manifest.
    pub strategy: String,
    /// Version amont, sans le `v` initial. Absente pour une strategie par commit,
    /// ou la version devient la date.
    pub version: Option<String>,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    /// Release publiee : le cas le plus confortable, seul a fournir un lien de
    /// changelog dans les propositions de mise a jour.
    Release,
    Tag,
    /// Ni release ni tag : la version sera la date du commit. Packageable, mais
    /// sans point de repere pour l'utilisateur.
    Commit,
}

/// Choisit la reference a packager.
///
/// L'ordre de preference suit la qualite du repere offert a l'administrateur
/// qui verra arriver les mises a jour.
pub fn choose(source: &SourceRef, releases: &[Release], tags: &[String]) -> SourceChoice {
    let forge = source.forge.autoupdate_slug();

    if let Some(r) = releases
        .iter()
        .find(|r| !r.prerelease && is_version_like(&r.tag))
    {
        return SourceChoice {
            url: crate::github::archive_url(source, &r.tag),
            version: Some(strip_v(&r.tag)),
            reference: r.tag.clone(),
            strategy: format!("latest_{forge}_release"),
            kind: SourceKind::Release,
        };
    }

    if let Some(tag) = latest_version_tag(tags) {
        return SourceChoice {
            url: crate::github::archive_url(source, &tag),
            version: Some(strip_v(&tag)),
            reference: tag,
            strategy: format!("latest_{forge}_tag"),
            kind: SourceKind::Tag,
        };
    }

    let reference = source
        .default_branch
        .clone()
        .unwrap_or_else(|| "main".into());
    SourceChoice {
        url: crate::github::archive_url(source, &reference),
        reference,
        strategy: format!("latest_{forge}_commit"),
        version: None,
        kind: SourceKind::Commit,
    }
}

/// Un tag exploitable ressemble a `x.y.z` ou `vx.y.z`, sans marqueur de
/// pre-version. Meme regle que l'autoupdater de YunoHost, pour que nos choix
/// et les siens ne divergent pas au premier cycle de mise a jour.
pub fn is_version_like(tag: &str) -> bool {
    const PRE: &[&str] = &[
        "rc", "beta", "alpha", "pre", "nightly", "snapshot", "dev", "test",
    ];
    let lower = tag.to_lowercase();
    if PRE.iter().any(|p| lower.contains(p)) {
        return false;
    }
    let core = lower.trim_start_matches('v');
    let mut parts = core.split('.');
    let first = parts.next().unwrap_or_default();
    !first.is_empty()
        && first.chars().all(|c| c.is_ascii_digit())
        && core.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn latest_version_tag(tags: &[String]) -> Option<String> {
    tags.iter()
        .filter(|t| is_version_like(t))
        .max_by(|a, b| version_key(a).cmp(&version_key(b)))
        .cloned()
}

/// Cle de tri numerique : `v1.10.0` doit passer apres `v1.9.0`, ce qu'un tri
/// lexicographique ferait a l'envers.
fn version_key(tag: &str) -> Vec<u64> {
    tag.trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

fn strip_v(tag: &str) -> String {
    tag.trim_start_matches('v').to_string()
}

/// Somme de controle de l'archive, telle qu'elle figurera dans le manifest.
pub fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Vrai si la forge sait appliquer une strategie de mise a jour automatique.
pub fn supports_autoupdate(forge: Forge) -> bool {
    matches!(
        forge,
        Forge::GitHub | Forge::GitLab | Forge::Gitea | Forge::Forgejo
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceRef {
        SourceRef {
            forge: Forge::GitHub,
            owner: "acme".into(),
            repo: "widget".into(),
            url: "https://github.com/acme/widget".into(),
            default_branch: Some("main".into()),
            commit: None,
        }
    }

    fn release(tag: &str, prerelease: bool) -> Release {
        Release {
            tag: tag.into(),
            prerelease,
            ..Default::default()
        }
    }

    #[test]
    fn une_release_stable_est_preferee_a_tout_le_reste() {
        let c = choose(&source(), &[release("v1.2.3", false)], &["v9.9.9".into()]);

        assert_eq!(c.kind, SourceKind::Release);
        assert_eq!(c.reference, "v1.2.3");
        assert_eq!(c.version.as_deref(), Some("1.2.3"));
        assert_eq!(c.strategy, "latest_github_release");
        assert!(c.url.ends_with("/archive/v1.2.3.tar.gz"));
    }

    #[test]
    fn une_preversion_n_est_pas_retenue() {
        let releases = [release("v2.0.0-rc1", true), release("v1.9.0", false)];
        let c = choose(&source(), &releases, &[]);
        assert_eq!(c.reference, "v1.9.0");
    }

    #[test]
    fn a_defaut_de_release_le_tag_de_version_le_plus_recent_gagne() {
        let tags = ["v1.9.0".into(), "v1.10.0".into(), "v1.2.0".into()];
        let c = choose(&source(), &[], &tags);

        assert_eq!(c.kind, SourceKind::Tag);
        // Un tri lexicographique aurait choisi v1.9.0.
        assert_eq!(c.reference, "v1.10.0");
        assert_eq!(c.strategy, "latest_github_tag");
    }

    #[test]
    fn sans_release_ni_tag_on_retombe_sur_la_branche_par_defaut() {
        let c = choose(&source(), &[], &[]);

        assert_eq!(c.kind, SourceKind::Commit);
        assert_eq!(c.reference, "main");
        assert_eq!(c.strategy, "latest_github_commit");
        // La version deviendra la date du commit : il n'y en a pas a annoncer.
        assert_eq!(c.version, None);
    }

    #[test]
    fn les_tags_fantaisistes_sont_ecartes() {
        assert!(is_version_like("v1.2.3"));
        assert!(is_version_like("1.2"));
        assert!(!is_version_like("v2.0.0-beta1"));
        assert!(!is_version_like("nightly"));
        assert!(!is_version_like("release-2024"));
        assert!(!is_version_like("latest"));
    }

    #[test]
    fn la_somme_de_controle_est_celle_du_manifest() {
        // Valeur de reference de sha256sum sur une chaine vide.
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(sha256(b"yunopack").len(), 64);
    }
}
