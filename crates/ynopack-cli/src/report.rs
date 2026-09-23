//! Restitution lisible des faits en terminal.
//!
//! Le rapport humain et le JSON portent la meme information : le premier sert a
//! decider, le second a enchainer. Aucun des deux n'est un resume de l'autre.

use ynp_core::facts::{Database, RepoFacts, Technology};

pub fn facts(f: &RepoFacts) -> String {
    let mut out = String::new();

    out.push_str(&format!("\n{}/{}\n", f.source.owner, f.source.repo));
    out.push_str(&format!("  {}\n\n", f.source.url));

    line(
        &mut out,
        "licence",
        f.meta
            .license_spdx
            .clone()
            .unwrap_or_else(|| "non identifiee".into()),
    );
    line(
        &mut out,
        "activite",
        match (&f.meta.pushed_at, f.meta.archived) {
            (_, true) => "depot ARCHIVE".to_string(),
            (Some(d), _) => format!("dernier push {}", &d[..10.min(d.len())]),
            (None, _) => "inconnue".into(),
        },
    );
    line(&mut out, "etoiles", f.meta.stars.to_string());

    out.push('\n');
    line(
        &mut out,
        "technologie",
        tech(f.stack.primary, f.stack.runtime_version.as_deref()),
    );
    if !f.stack.package_managers.is_empty() {
        line(
            &mut out,
            "gestionnaires",
            f.stack.package_managers.join(", "),
        );
    }
    if let Some(b) = &f.stack.build_script {
        line(&mut out, "build declare", b.clone());
    }
    line(
        &mut out,
        "base de donnees",
        database(f.services.database, f.services.database_evidence.as_deref()),
    );
    if f.services.needs_redis {
        line(&mut out, "cache", "redis".into());
    }

    out.push('\n');
    match &f.build {
        None => line(
            &mut out,
            "Dockerfile",
            "absent — la detection s'appuie sur les seuls fichiers de projet".into(),
        ),
        Some(b) => {
            line(&mut out, "Dockerfile", b.dockerfile_path.clone());
            line(
                &mut out,
                "  etapes",
                format!("{} ({} de build)", b.stages.len(), b.build_steps.len()),
            );
            if !b.apt_packages.is_empty() {
                line(&mut out, "  apt", liste(&b.apt_packages));
            }
            if !b.apk_packages.is_empty() {
                line(
                    &mut out,
                    "  apk",
                    format!("{} (a traduire vers deb)", liste(&b.apk_packages)),
                );
            }
            if !b.expose.is_empty() {
                line(
                    &mut out,
                    "  port",
                    b.expose
                        .iter()
                        .map(u16::to_string)
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            if let Some(cmd) = b.start_command() {
                line(&mut out, "  demarrage", cmd);
            }
        }
    }

    if let Some(c) = &f.compose {
        let apps: Vec<&str> = c
            .services
            .iter()
            .filter(|s| s.is_app)
            .map(|s| s.name.as_str())
            .collect();
        out.push('\n');
        line(&mut out, "compose", c.path.clone());
        line(
            &mut out,
            "  service app",
            if apps.is_empty() {
                "aucun identifie".into()
            } else {
                apps.join(", ")
            },
        );
    }

    if !f.config.variables.is_empty() {
        out.push('\n');
        line(
            &mut out,
            "configuration",
            f.config
                .example_file
                .clone()
                .unwrap_or_else(|| "compose".into()),
        );
        // Le nombre brut peut etre enorme (335 pour linkwarden, une serie par
        // fournisseur OIDC). Ce qui compte est le nombre de variables dont le
        // role est reconnu : ce sont les seules a cabler sur YunoHost.
        let reconnues = f
            .config
            .variables
            .iter()
            .filter(|v| v.role != ynp_core::facts::ConfigRole::Other)
            .count();
        line(
            &mut out,
            "  variables",
            format!(
                "{} dont {reconnues} avec un role reconnu, {} secrets",
                f.config.variables.len(),
                f.config.variables.iter().filter(|v| v.secret).count()
            ),
        );
    }

    if !f.stack.native_deps.is_empty() {
        out.push('\n');
        line(
            &mut out,
            "compilation",
            format!("modules natifs : {}", liste(&f.stack.native_deps)),
        );
    }
    if !f.services.unsupported.is_empty() {
        line(
            &mut out,
            "sans equivalent",
            f.services.unsupported.join(", "),
        );
    }
    if f.services.requires_container_runtime {
        line(
            &mut out,
            "ATTENTION",
            "distribue uniquement sous forme d'image".into(),
        );
    }

    out
}

/// Une ligne du rapport : libelle aligne, puis valeur.
fn line(out: &mut String, label: &str, value: String) {
    out.push_str(&format!("  {label:<16}{value}\n"));
}

fn tech(t: Technology, version: Option<&str>) -> String {
    let name = match t {
        Technology::Php => "PHP",
        Technology::NodeJs => "Node.js",
        Technology::Python => "Python",
        Technology::Go => "Go",
        Technology::Ruby => "Ruby",
        Technology::Rust => "Rust",
        Technology::Java => "Java",
        Technology::Static => "fichiers statiques",
        Technology::Unknown => return "non identifiee".into(),
    };
    match version {
        Some(v) => format!("{name} {v}"),
        None => name.to_string(),
    }
}

fn database(db: Database, evidence: Option<&str>) -> String {
    let name = match db {
        Database::None => return "aucune".into(),
        Database::MySql => "MySQL/MariaDB",
        Database::PostgreSql => "PostgreSQL",
        Database::Sqlite => "SQLite (rien a provisionner)",
        Database::MongoDb => "MongoDB",
        Database::Unsupported => "non supportee",
    };
    match evidence {
        Some(e) => format!("{name}  [{e}]"),
        None => name.to_string(),
    }
}

/// Tronque une liste longue plutot que d'inonder le terminal.
fn liste(items: &[String]) -> String {
    const MAX: usize = 6;
    if items.len() <= MAX {
        return items.join(", ");
    }
    format!("{}, … (+{})", items[..MAX].join(", "), items.len() - MAX)
}
