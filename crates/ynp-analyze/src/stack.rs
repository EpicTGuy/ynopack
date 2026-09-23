//! Detection de la technologie et de la version de runtime.
//!
//! Deux sources, par ordre de fiabilite decroissante : l'image de base du
//! Dockerfile — qui dit exactement ce sur quoi l'upstream fait tourner son
//! application — puis les fichiers de projet, qui expriment une contrainte
//! (`>=18`) plutot qu'un choix.

use crate::knowledge;
use serde_json::Value;
use ynp_core::facts::{BuildRecipe, StackFacts, Technology};
use ynp_core::tree::RepoTree;

pub fn detect(tree: &RepoTree, build: Option<&BuildRecipe>) -> StackFacts {
    let mut facts = StackFacts::default();

    // L'image de runtime prime : c'est un fait, pas une contrainte.
    if let Some(stage) = build.and_then(|b| b.runtime_stage()) {
        if let Some((tech, version)) = from_base_image(&stage.image, stage.tag.as_deref()) {
            facts.primary = tech;
            facts.runtime_version = version;
        }
    }

    // Les fichiers de projet completent, et prennent la main si le Dockerfile
    // n'a rien appris (image `scratch`, base inconnue, absence de Dockerfile).
    let from_files = from_project_files(tree);
    if facts.primary == Technology::Unknown {
        facts.primary = from_files.primary;
    }
    // Un tag d'image peut etre plus vague que le fichier de projet : `golang:1`
    // face a `go 1.26.0`. On garde la version la plus precise des deux.
    facts.runtime_version = more_precise(facts.runtime_version.take(), from_files.runtime_version);
    facts.package_managers = from_files.package_managers;
    facts.has_lockfile = from_files.has_lockfile;
    facts.build_script = from_files.build_script;
    facts.native_deps = native_deps(tree);

    facts
}

/// Retient la version la plus precise, c'est-a-dire celle qui porte le plus de
/// composantes. A precision egale, celle de l'image de base fait foi : c'est un
/// fait, quand le fichier de projet n'exprime qu'une contrainte.
fn more_precise(from_image: Option<String>, from_files: Option<String>) -> Option<String> {
    match (from_image, from_files) {
        (Some(image), Some(files)) => {
            let parts = |v: &str| v.matches('.').count();
            if parts(&files) > parts(&image) {
                Some(files)
            } else {
                Some(image)
            }
        }
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    }
}

/// Technologie et version deduites d'une image de base Docker.
fn from_base_image(image: &str, tag: Option<&str>) -> Option<(Technology, Option<String>)> {
    let short = image.rsplit('/').next().unwrap_or(image).to_lowercase();
    let version = tag.and_then(first_version);

    let tech = match short.as_str() {
        "node" | "nodejs" => Technology::NodeJs,
        "python" => Technology::Python,
        "php" | "php-fpm" => Technology::Php,
        "golang" | "go" => Technology::Go,
        "ruby" => Technology::Ruby,
        "rust" => Technology::Rust,
        "openjdk" | "eclipse-temurin" | "amazoncorretto" => Technology::Java,
        "nginx" | "httpd" | "caddy" => Technology::Static,
        // `scratch` et les bases nues ne disent rien de la technologie : c'est
        // aux fichiers de projet de trancher.
        _ => return None,
    };
    Some((tech, version))
}

/// Technologie deduite de la presence de fichiers de projet.
///
/// L'ordre reflete la specificite : un depot qui a un `go.mod` est un projet
/// Go, meme s'il contient aussi un `package.json` pour son frontal.
fn from_project_files(tree: &RepoTree) -> StackFacts {
    let mut f = StackFacts::default();

    if tree.has("go.mod") {
        f.primary = Technology::Go;
        f.runtime_version = go_version(tree);
    } else if tree.has("composer.json") {
        f.primary = Technology::Php;
        f.runtime_version = php_version(tree);
    } else if tree.has("Gemfile") {
        f.primary = Technology::Ruby;
        f.runtime_version = ruby_version(tree);
    } else if tree.has("Cargo.toml") {
        f.primary = Technology::Rust;
    } else if tree.has("pyproject.toml") || tree.has("requirements.txt") || tree.has("setup.py") {
        f.primary = Technology::Python;
        f.runtime_version = python_version(tree);
    } else if tree.has("package.json") {
        f.primary = Technology::NodeJs;
        f.runtime_version = node_version(tree);
    } else if tree.has("pom.xml") || tree.has("build.gradle") || tree.has("build.gradle.kts") {
        f.primary = Technology::Java;
    } else if tree.has("index.html") {
        f.primary = Technology::Static;
    }

    // Un depot Go ou Python peut aussi embarquer un frontal Node : le
    // gestionnaire de paquets et l'etape de build restent utiles a connaitre.
    if tree.has("package.json") {
        if f.runtime_version.is_none() && f.primary == Technology::NodeJs {
            f.runtime_version = node_version(tree);
        }
        f.build_script = build_script(tree);
    }
    f.package_managers = package_managers(tree);
    f.has_lockfile = !f.package_managers.is_empty();
    f
}

fn package_managers(tree: &RepoTree) -> Vec<String> {
    let mut out = Vec::new();
    for (file, name) in [
        ("pnpm-lock.yaml", "pnpm"),
        ("yarn.lock", "yarn"),
        ("bun.lockb", "bun"),
        ("package-lock.json", "npm"),
        ("composer.lock", "composer"),
        ("poetry.lock", "poetry"),
        ("uv.lock", "uv"),
        ("Pipfile.lock", "pipenv"),
        ("Gemfile.lock", "bundler"),
        ("go.sum", "go"),
        ("Cargo.lock", "cargo"),
    ] {
        if tree.has(file) {
            out.push(name.to_string());
        }
    }
    out
}

fn json(tree: &RepoTree, path: &str) -> Option<Value> {
    serde_json::from_str(tree.text(path)?).ok()
}

fn build_script(tree: &RepoTree) -> Option<String> {
    let pkg = json(tree, "package.json")?;
    let scripts = pkg.get("scripts")?;
    // `build` d'abord, puis les variantes de production les plus repandues.
    for key in [
        "build",
        "build:prod",
        "build:production",
        "compile",
        "prepare",
    ] {
        if let Some(Value::String(s)) = scripts.get(key) {
            if !s.trim().is_empty() {
                return Some(format!("npm run {key}"));
            }
        }
    }
    None
}

/// Modules npm compilant du code natif, qui exigent des paquets Debian absents
/// du package.json. C'est la cause la plus frequente d'un `npm ci` en echec
/// sur une instance YunoHost.
fn native_deps(tree: &RepoTree) -> Vec<String> {
    let Some(pkg) = json(tree, "package.json") else {
        return Vec::new();
    };
    let k = knowledge::get();

    let mut out = Vec::new();
    for section in ["dependencies", "devDependencies", "optionalDependencies"] {
        let Some(Value::Object(deps)) = pkg.get(section) else {
            continue;
        };
        for name in deps.keys() {
            if !k.npm_native_deps(name).is_empty() && !out.contains(name) {
                out.push(name.clone());
            }
        }
    }
    out
}

// --- Extraction de version, par ecosysteme ---

/// Premiere version `X` ou `X.Y` trouvee dans une chaine.
///
/// Les contraintes amont prennent toutes les formes : `>=18`, `^8.2`, `~3.11`,
/// `20-bookworm-slim`, `1.22.3`. On en retient le debut, qui est ce qui
/// interesse les resources du manifest.
fn first_version(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|c| c.is_ascii_digit())?;
    let mut end = start;
    let mut dots = 0;
    while end < bytes.len() {
        match bytes[end] {
            b'0'..=b'9' => end += 1,
            b'.' if dots < 1 && end + 1 < bytes.len() && bytes[end + 1].is_ascii_digit() => {
                dots += 1;
                end += 1;
            }
            _ => break,
        }
    }
    Some(s[start..end].to_string())
}

fn node_version(tree: &RepoTree) -> Option<String> {
    if let Some(v) = tree.text(".nvmrc").and_then(first_version) {
        return Some(v);
    }
    let pkg = json(tree, "package.json")?;
    pkg.get("engines")?
        .get("node")?
        .as_str()
        .and_then(first_version)
}

fn go_version(tree: &RepoTree) -> Option<String> {
    tree.text("go.mod")?
        .lines()
        .find_map(|l| l.trim().strip_prefix("go "))
        .and_then(first_version)
}

fn php_version(tree: &RepoTree) -> Option<String> {
    let c = json(tree, "composer.json")?;
    c.get("require")?
        .get("php")?
        .as_str()
        .and_then(first_version)
}

fn ruby_version(tree: &RepoTree) -> Option<String> {
    if let Some(v) = tree.text(".ruby-version").and_then(first_version) {
        return Some(v);
    }
    tree.text("Gemfile")?
        .lines()
        .find(|l| l.trim_start().starts_with("ruby "))
        .and_then(first_version)
}

fn python_version(tree: &RepoTree) -> Option<String> {
    if let Some(v) = tree.text(".python-version").and_then(first_version) {
        return Some(v);
    }
    tree.text("pyproject.toml")?
        .lines()
        .find(|l| l.trim_start().starts_with("requires-python"))
        .and_then(first_version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::facts::BuildStage;

    fn recipe(image: &str, tag: Option<&str>) -> BuildRecipe {
        BuildRecipe {
            stages: vec![BuildStage {
                image: image.into(),
                tag: tag.map(str::to_string),
                alias: None,
            }],
            ..Default::default()
        }
    }

    #[test]
    fn l_image_de_base_donne_la_technologie_et_la_version() {
        let t = RepoTree::new();
        let f = detect(&t, Some(&recipe("node", Some("20-bookworm-slim"))));
        assert_eq!(f.primary, Technology::NodeJs);
        assert_eq!(f.runtime_version.as_deref(), Some("20"));
    }

    #[test]
    fn une_base_muette_laisse_les_fichiers_de_projet_trancher() {
        // Vikunja : binaire Go statique sur `scratch`.
        let t = RepoTree::from_pairs([("go.mod", "module x\n\ngo 1.22\n")]);
        let f = detect(&t, Some(&recipe("scratch", None)));
        assert_eq!(f.primary, Technology::Go);
        assert_eq!(f.runtime_version.as_deref(), Some("1.22"));
    }

    #[test]
    fn un_projet_go_avec_un_frontal_node_reste_un_projet_go() {
        let t = RepoTree::from_pairs([
            ("go.mod", "go 1.21"),
            ("package.json", r#"{"scripts":{"build":"vite build"}}"#),
            ("pnpm-lock.yaml", ""),
        ]);
        let f = detect(&t, None);
        assert_eq!(f.primary, Technology::Go);
        // L'etape de build du frontal reste connue : il faudra l'executer.
        assert_eq!(f.build_script.as_deref(), Some("npm run build"));
        assert_eq!(f.package_managers, vec!["pnpm"]);
    }

    #[test]
    fn les_contraintes_de_version_sont_normalisees() {
        assert_eq!(first_version(">=18.17.0").as_deref(), Some("18.17"));
        assert_eq!(first_version("^8.2").as_deref(), Some("8.2"));
        assert_eq!(first_version("20-alpine").as_deref(), Some("20"));
        assert_eq!(first_version("3.12-slim-bookworm").as_deref(), Some("3.12"));
        assert_eq!(first_version("latest"), None);
    }

    #[test]
    fn chaque_ecosysteme_expose_sa_version() {
        let php = RepoTree::from_pairs([("composer.json", r#"{"require":{"php":"^8.2"}}"#)]);
        assert_eq!(detect(&php, None).runtime_version.as_deref(), Some("8.2"));

        let py = RepoTree::from_pairs([("pyproject.toml", "requires-python = \">=3.11\"\n")]);
        assert_eq!(detect(&py, None).runtime_version.as_deref(), Some("3.11"));

        let rb = RepoTree::from_pairs([("Gemfile", "source 'x'\nruby \"3.2.2\"\n")]);
        assert_eq!(detect(&rb, None).runtime_version.as_deref(), Some("3.2"));

        let node = RepoTree::from_pairs([(".nvmrc", "20.11.0\n"), ("package.json", "{}")]);
        assert_eq!(
            detect(&node, None).runtime_version.as_deref(),
            Some("20.11")
        );
    }

    #[test]
    fn les_modules_npm_a_compilation_native_sont_signales() {
        let t = RepoTree::from_pairs([(
            "package.json",
            r#"{"dependencies":{"sharp":"^0.33","lodash":"^4"},"devDependencies":{"canvas":"^2"}}"#,
        )]);
        let f = detect(&t, None);
        assert_eq!(f.native_deps, vec!["sharp", "canvas"]);
    }

    #[test]
    fn un_depot_sans_aucun_indice_reste_inconnu_plutot_que_devine() {
        let f = detect(&RepoTree::from_pairs([("README.md", "# projet")]), None);
        assert_eq!(f.primary, Technology::Unknown);
        assert_eq!(f.runtime_version, None);
    }
}

#[cfg(test)]
mod precision {
    use super::*;

    #[test]
    fn la_version_la_plus_precise_l_emporte() {
        // Cas reel de miniflux : `golang:1` dans le Dockerfile de
        // conditionnement, `go 1.26.0` dans go.mod.
        assert_eq!(
            more_precise(Some("1".into()), Some("1.26".into())).as_deref(),
            Some("1.26")
        );
        // A precision egale, l'image de base fait foi.
        assert_eq!(
            more_precise(Some("20".into()), Some("18".into())).as_deref(),
            Some("20")
        );
        assert_eq!(
            more_precise(None, Some("3.11".into())).as_deref(),
            Some("3.11")
        );
        assert_eq!(
            more_precise(Some("8.2".into()), None).as_deref(),
            Some("8.2")
        );
    }
}
