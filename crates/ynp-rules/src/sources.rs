//! Regles sur la source a packager.
//!
//! Le manifest exige un `sha256`. La question n'est donc pas « y a-t-il du
//! code » mais « y a-t-il une reference stable a figer ».

use crate::Rule;
use ynp_core::facts::RepoFacts;
use ynp_core::finding::{Evidence, Finding, Severity};

pub struct AucuneSource;

impl Rule for AucuneSource {
    fn id(&self) -> &'static str {
        "SRC001"
    }

    fn check(&self, facts: &RepoFacts) -> Option<Finding> {
        // Une branche par defaut suffit a produire une archive : c'est le
        // dernier recours, mais c'en est un.
        if facts.source.default_branch.is_some() {
            return None;
        }
        Some(
            Finding::new(
                self.id(),
                Severity::Blocker,
                "Aucune archive telechargeable",
            )
            .detail(
                "Ni release, ni tag, ni branche par defaut : il n'y a rien a figer dans \
                     `[resources.sources]`, dont l'url et le sha256 sont obligatoires.",
            )
            .remediation("Verifier que le depot n'est pas vide et qu'il est bien public."),
        )
    }
}

pub struct PasDeReperePourLesMisesAJour;

impl Rule for PasDeReperePourLesMisesAJour {
    fn id(&self) -> &'static str {
        "SRC002"
    }

    fn check(&self, facts: &RepoFacts) -> Option<Finding> {
        let a_une_release = facts
            .releases
            .iter()
            .any(|r| !r.prerelease && est_une_version(&r.tag));
        let a_un_tag = facts.tags.iter().any(|t| est_une_version(t));

        if a_une_release || a_un_tag {
            return None;
        }

        Some(
            Finding::new(self.id(), Severity::Major, "Ni release ni tag de version")
                .detail(
                    "L'application reste packageable avec la strategie \
                     `latest_github_commit`, mais sa version sera la date du commit : \
                     l'administrateur n'aura aucun repere pour savoir ce qu'il installe, \
                     et les propositions de mise a jour n'auront pas de journal des \
                     modifications.",
                )
                .remediation(
                    "Verifier si l'amont publie ses versions ailleurs. A defaut, accepter \
                     `autoupdate.strategy = \"latest_github_commit\"` en connaissance de cause.",
                )
                .evidence(Evidence {
                    file: "(metadonnees de la forge)".into(),
                    line: None,
                    excerpt: Some(format!(
                        "{} release(s), {} tag(s), aucun exploitable",
                        facts.releases.len(),
                        facts.tags.len()
                    )),
                }),
        )
    }
}

/// Meme regle que l'autoupdater de YunoHost, pour que nos choix et les siens
/// ne divergent pas au premier cycle de mise a jour.
fn est_une_version(tag: &str) -> bool {
    const PRE: &[&str] = &[
        "rc", "beta", "alpha", "pre", "nightly", "snapshot", "dev", "test",
    ];
    let lower = tag.to_lowercase();
    if PRE.iter().any(|p| lower.contains(p)) {
        return false;
    }
    let core = lower.trim_start_matches('v');
    let first = core.split('.').next().unwrap_or_default();
    !first.is_empty()
        && first.chars().all(|c| c.is_ascii_digit())
        && core.chars().all(|c| c.is_ascii_digit() || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::depot_sain;
    use ynp_core::facts::Release;

    #[test]
    fn une_release_stable_satisfait_les_deux_regles() {
        let f = depot_sain();
        assert!(AucuneSource.check(&f).is_none());
        assert!(PasDeReperePourLesMisesAJour.check(&f).is_none());
    }

    #[test]
    fn un_depot_sans_branche_par_defaut_est_bloque() {
        let mut f = depot_sain();
        f.source.default_branch = None;
        assert_eq!(AucuneSource.check(&f).unwrap().severity, Severity::Blocker);
    }

    #[test]
    fn l_absence_de_repere_degrade_sans_refuser() {
        // La strategie par commit existe : refuser serait excessif.
        let mut f = depot_sain();
        f.releases.clear();
        f.tags.clear();

        let finding = PasDeReperePourLesMisesAJour.check(&f).unwrap();
        assert_eq!(finding.severity, Severity::Major);
        assert!(AucuneSource.check(&f).is_none());
    }

    #[test]
    fn un_tag_de_version_suffit_a_faire_un_repere() {
        let mut f = depot_sain();
        f.releases.clear();
        f.tags = vec!["v2.1.0".into()];
        assert!(PasDeReperePourLesMisesAJour.check(&f).is_none());
    }

    #[test]
    fn une_preversion_seule_ne_fait_pas_un_repere() {
        let mut f = depot_sain();
        f.releases = vec![Release {
            tag: "v2.0.0-rc1".into(),
            prerelease: true,
            ..Default::default()
        }];
        f.tags = vec!["nightly".into()];
        assert!(PasDeReperePourLesMisesAJour.check(&f).is_some());
    }
}
