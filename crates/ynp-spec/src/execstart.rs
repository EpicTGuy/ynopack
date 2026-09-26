//! Traduction d'une commande de demarrage Docker en `ExecStart` systemd.
//!
//! Docker lance l'application depuis une arborescence qu'il maitrise ; YunoHost
//! la lance depuis `$install_dir`, avec des runtimes provisionnes a des chemins
//! qu'il expose par des variables. La traduction consiste donc surtout a
//! replacer les chemins.
//!
//! Les placeholders `__MAJUSCULES__` sont remplaces par les helpers au moment
//! ou la configuration est installee : c'est le mecanisme natif de YunoHost.

use ynp_core::facts::{RepoFacts, Technology};
use ynp_core::known::Known;

/// Repertoires ou une image Docker range son application.
const RACINES: &[&str] = &[
    "/usr/local/bin/",
    "/usr/bin/",
    "/app/",
    "/srv/",
    "/opt/",
    "/home/app/",
    "/usr/src/app/",
];

pub fn derive(facts: &RepoFacts, app_id: &str) -> Known<String> {
    // Un binaire nu publie par l'amont est renomme d'apres l'app : son chemin
    // est connu sans qu'il faille interpreter quoi que ce soit.
    if let Some(sel) = &facts.selection {
        if sel.prebuilt.iter().any(|a| !a.extract) {
            return Known::resolved(format!("__INSTALL_DIR__/{app_id}"));
        }
    }

    let Some(build) = &facts.build else {
        return Known::unresolved(
            "aucun Dockerfile d'ou tirer la commande de demarrage",
            &[
                "README.md",
                "Procfile",
                "la documentation du projet d'origine",
            ],
        );
    };
    let Some(commande) = build.start_command() else {
        return Known::unresolved(
            "le Dockerfile ne declare ni CMD ni ENTRYPOINT",
            &[&build.dockerfile_path, "README.md", "Procfile"],
        );
    };

    match traduire(&commande, facts.stack.primary) {
        Some(execstart) => Known::resolved(execstart),
        None => Known::unresolved(
            format!("commande « {commande} » non transposable automatiquement"),
            &[
                &build.dockerfile_path,
                "la documentation du projet d'origine",
            ],
        ),
    }
}

fn traduire(commande: &str, tech: Technology) -> Option<String> {
    let mots: Vec<&str> = commande.split_whitespace().collect();
    let premier = *mots.first()?;

    // Un script d'entree enveloppe le vrai demarrage : on ne sait pas ce qu'il
    // fait, et le deviner serait exactement ce que le projet refuse.
    if premier.ends_with(".sh") || premier.contains("entrypoint") || premier == "/bin/sh" {
        return None;
    }

    let reste = |depuis: usize| -> String {
        mots[depuis..]
            .iter()
            .map(|m| chemin(m))
            .collect::<Vec<_>>()
            .join(" ")
    };

    match premier {
        // L'interprete vient de la resource du manifest, pas du systeme.
        "node" => Some(format!("__NODEJS_DIR__/node {}", reste(1))),
        "npm" | "yarn" | "pnpm" => None, // demarrer par un gestionnaire de paquets est deconseille
        "ruby" | "bundle" => Some(format!("__RUBY_DIR__/{premier} {}", reste(1))),
        "php" | "php-fpm" => None, // pris en charge par la brique phpfpm
        "python" | "python3" => {
            // Pas de resource Python cote YunoHost : l'interprete vient d'un
            // environnement virtuel que le script d'installation cree.
            Some(format!("__INSTALL_DIR__/venv/bin/python {}", reste(1)))
        }
        _ => {
            let binaire = chemin(premier);
            // Un binaire hors des racines connues n'est pas localisable.
            if !binaire.starts_with("__INSTALL_DIR__") {
                if tech == Technology::Go || tech == Technology::Rust {
                    // Un executable compile nomme sans chemin se trouve dans
                    // le repertoire d'installation.
                    return Some(format!("__INSTALL_DIR__/{}", nom_de_fichier(premier)));
                }
                return None;
            }
            Some(if mots.len() > 1 {
                format!("{binaire} {}", reste(1))
            } else {
                binaire
            })
        }
    }
}

/// Replace un chemin de conteneur dans le repertoire d'installation.
fn chemin(mot: &str) -> String {
    if mot.starts_with('-') {
        return mot.to_string();
    }
    for racine in RACINES {
        if let Some(reste) = mot.strip_prefix(racine) {
            return format!("__INSTALL_DIR__/{reste}");
        }
    }
    if let Some(reste) = mot.strip_prefix("./") {
        return format!("__INSTALL_DIR__/{reste}");
    }
    mot.to_string()
}

fn nom_de_fichier(chemin: &str) -> &str {
    chemin.rsplit('/').next().unwrap_or(chemin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::facts::{ArchAsset, BuildRecipe, SourceSelection, StackFacts};

    fn facts_avec(cmd: Option<Vec<String>>, tech: Technology) -> RepoFacts {
        RepoFacts {
            stack: StackFacts {
                primary: tech,
                ..Default::default()
            },
            build: Some(BuildRecipe {
                dockerfile_path: "Dockerfile".into(),
                cmd,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn un_binaire_publie_est_localise_sans_interpretation() {
        // Cas de miniflux et gotify : l'amont publie un executable, renomme
        // d'apres l'app a l'installation.
        let mut f = facts_avec(None, Technology::Go);
        f.selection = Some(SourceSelection {
            prebuilt: vec![ArchAsset {
                arch: "amd64".into(),
                extract: false,
                ..Default::default()
            }],
            ..Default::default()
        });
        assert_eq!(
            derive(&f, "miniflux").value().unwrap(),
            "__INSTALL_DIR__/miniflux"
        );
    }

    #[test]
    fn un_demarrage_node_passe_par_la_resource_du_manifest() {
        let f = facts_avec(
            Some(vec!["node".into(), "./dist/main.js".into()]),
            Technology::NodeJs,
        );
        assert_eq!(
            derive(&f, "app").value().unwrap(),
            "__NODEJS_DIR__/node __INSTALL_DIR__/dist/main.js"
        );
    }

    #[test]
    fn un_chemin_de_conteneur_est_replace_dans_le_repertoire_d_installation() {
        let f = facts_avec(
            Some(vec!["/usr/local/bin/buzz-relay".into()]),
            Technology::Rust,
        );
        assert_eq!(
            derive(&f, "buzz").value().unwrap(),
            "__INSTALL_DIR__/buzz-relay"
        );
    }

    #[test]
    fn un_script_d_entree_n_est_pas_devine() {
        // On ne sait pas ce que fait entrypoint.sh, et l'inventer serait
        // exactement ce que le projet refuse.
        let f = facts_avec(
            Some(vec!["/usr/local/memos/entrypoint.sh".into()]),
            Technology::Go,
        );
        let r = derive(&f, "memos");
        assert!(!r.is_resolved());
        assert!(r.reason().unwrap().unknown.contains("non transposable"));
    }

    #[test]
    fn l_absence_de_cmd_est_signalee_avec_ou_chercher() {
        let f = facts_avec(None, Technology::NodeJs);
        let r = derive(&f, "app");
        assert!(!r.is_resolved());
        assert!(r.reason().unwrap().look_in.iter().any(|s| s == "Procfile"));
    }

    #[test]
    fn un_executable_compile_sans_chemin_est_cherche_dans_l_installation() {
        let f = facts_avec(
            Some(vec!["gotify-app".into(), "serve".into()]),
            Technology::Go,
        );
        assert_eq!(
            derive(&f, "gotify").value().unwrap(),
            "__INSTALL_DIR__/gotify-app"
        );
    }

    #[test]
    fn php_fpm_n_est_pas_un_service_systemd() {
        // La brique phpfpm de YunoHost s'en charge.
        let f = facts_avec(Some(vec!["php-fpm".into()]), Technology::Php);
        assert!(!derive(&f, "app").is_resolved());
    }
}
