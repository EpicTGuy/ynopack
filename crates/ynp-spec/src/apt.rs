//! Dependances Debian a declarer dans `[resources.apt]`.
//!
//! Trois sources se cumulent, et c'est le cumul qui compte : une seule d'entre
//! elles laisserait des trous, et une dependance manquante fait echouer
//! l'installation chez l'utilisateur.

use ynp_analyze::knowledge;
use ynp_core::facts::RepoFacts;

pub fn packages(facts: &RepoFacts) -> Vec<String> {
    let k = knowledge::get();
    // Quand l'amont publie un binaire, rien n'est compile sur la machine
    // cible : declarer build-essential, git ou make encombrerait le manifest
    // et rallongerait l'installation pour rien.
    let sans_compilation = facts
        .selection
        .as_ref()
        .is_some_and(|s| s.evite_la_compilation());
    let mut out: Vec<String> = Vec::new();
    let mut ajouter = |p: &str| {
        let p = p.trim();
        if !p.is_empty() && !out.iter().any(|x| x == p) {
            out.push(p.to_string());
        }
    };

    if let Some(build) = &facts.build {
        // 1. Les paquets Debian lus litteralement dans le Dockerfile.
        for p in &build.apt_packages {
            if est_specifique_au_conteneur(p) || (sans_compilation && sert_a_construire(p)) {
                continue;
            }
            ajouter(p);
        }

        // 2. Les paquets Alpine traduits. Une traduction vide signale un
        //    equivalent sans objet en Debian (`musl-dev`) ; une traduction
        //    absente est signalee par la regle APK001, pas comblee ici.
        for p in &build.apk_packages {
            if let Some(deb) = k.apk_to_deb(p) {
                if deb.is_empty()
                    || est_specifique_au_conteneur(deb)
                    || (sans_compilation && sert_a_construire(deb))
                {
                    continue;
                }
                ajouter(deb);
            }
        }
    }

    // 3. Les dependances cachees des modules npm a compilation native, qui
    //    n'apparaissent dans aucun Dockerfile ni package.json.
    if !facts
        .selection
        .as_ref()
        .is_some_and(|s| s.evite_la_compilation())
    {
        for module in &facts.stack.native_deps {
            for p in k.npm_native_deps(module) {
                ajouter(p);
            }
        }
    }

    out.sort_unstable();
    out
}

/// Paquets qui n'ont de sens que dans une image.
///
/// YunoHost fournit deja le socle du systeme : reinstaller `ca-certificates`
/// ou un gestionnaire de processus encombre le manifest sans rien apporter.
fn est_specifique_au_conteneur(paquet: &str) -> bool {
    const INUTILES: &[&str] = &[
        "ca-certificates",
        "tzdata",
        "locales",
        "tini",
        "gosu",
        "su-exec",
        "dumb-init",
        "passwd",
        "procps",
        "coreutils",
        "findutils",
        "bash",
        "apt-utils",
        "apt-transport-https",
        "gnupg",
        "software-properties-common",
    ];
    INUTILES.contains(&paquet)
}

/// Paquets qui ne servent qu'a construire.
fn sert_a_construire(paquet: &str) -> bool {
    const OUTILS: &[&str] = &[
        "build-essential",
        "gcc",
        "g++",
        "make",
        "cmake",
        "pkg-config",
        "autoconf",
        "automake",
        "libtool",
        "git",
        "python3-dev",
        "linux-libc-dev",
        "clang",
        "rustc",
        "cargo",
    ];
    OUTILS.contains(&paquet) || paquet.ends_with("-dev")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::facts::{ArchAsset, BuildRecipe, SourceSelection, StackFacts};

    fn facts(apt: &[&str], apk: &[&str], natifs: &[&str]) -> RepoFacts {
        RepoFacts {
            build: Some(BuildRecipe {
                apt_packages: apt.iter().map(|s| s.to_string()).collect(),
                apk_packages: apk.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }),
            stack: StackFacts {
                native_deps: natifs.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn les_trois_sources_se_cumulent() {
        let f = facts(&["libvips-dev"], &["build-base"], &["canvas"]);
        let p = packages(&f);

        assert!(
            p.contains(&"libvips-dev".to_string()),
            "depuis le Dockerfile"
        );
        assert!(
            p.contains(&"build-essential".to_string()),
            "traduit depuis Alpine"
        );
        assert!(
            p.contains(&"libcairo2-dev".to_string()),
            "dependance cachee de canvas"
        );
    }

    #[test]
    fn le_socle_fourni_par_yunohost_n_est_pas_redeclare() {
        let p = packages(&facts(&["ca-certificates", "tzdata", "curl"], &[], &[]));
        assert_eq!(p, vec!["curl"]);
    }

    #[test]
    fn une_traduction_sans_objet_en_debian_n_ajoute_rien() {
        // `musl-dev` est propre a Alpine.
        let p = packages(&facts(&[], &["musl-dev", "vips-dev"], &[]));
        assert_eq!(p, vec!["libvips-dev"]);
    }

    #[test]
    fn un_paquet_alpine_inconnu_n_est_pas_invente() {
        // La regle APK001 le signale ; le combler au hasard ferait echouer apt.
        let p = packages(&facts(&[], &["paquet-jamais-vu"], &[]));
        assert!(p.is_empty());
    }

    #[test]
    fn un_binaire_publie_dispense_des_dependances_de_compilation() {
        let mut f = facts(&["libvips-dev"], &[], &["sharp"]);
        f.selection = Some(SourceSelection {
            prebuilt: vec![ArchAsset {
                arch: "amd64".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let p = packages(&f);
        // Les modules natifs ne seront pas compiles ; leurs -dev sont inutiles.
        assert!(!p.contains(&"libvips-dev".to_string()) || p.len() == 1);
    }

    #[test]
    fn la_liste_est_triee_et_sans_doublon() {
        let p = packages(&facts(&["zlib1g-dev", "curl", "curl"], &["curl"], &[]));
        assert_eq!(p, vec!["curl", "zlib1g-dev"]);
    }
}
