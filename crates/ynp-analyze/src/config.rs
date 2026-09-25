//! Variables de configuration de l'application.
//!
//! Un `.env.example` dit ce que l'application attend pour demarrer. Classer
//! chaque variable par role — port, adresse de base, identifiants — est ce qui
//! permet de cabler la configuration sur les valeurs YunoHost (`$port`,
//! `$domain`, `$db_pwd`) sans rien deviner.
//!
//! Les sources sont lues de la plus sure a la moins sure, et la premiere qui
//! nomme une variable l'emporte. Chacune retient d'ou elle vient : une
//! proposition faite a l'humain vaut surtout par sa provenance.

use crate::knowledge;
use ynp_core::facts::{BuildRecipe, ComposeFacts, ConfigFacts, ConfigVar};
use ynp_core::tree::RepoTree;

/// Fichiers d'exemple reconnus par leur chemin exact, par ordre de preference.
const CANDIDATES: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.dist",
    "env.example",
    ".env.defaults",
    "example.env",
    "config/.env.example",
];

/// Terminaisons d'un fichier d'exemple au format `.env`.
///
/// Une liste de chemins exacts ne suffit pas : beaucoup de projets prefixent le
/// fichier du nom de l'application. gotify publie `gotify-server.env.example`,
/// qui documente son port et sa base — deux champs que l'outil declarait
/// indeterminables faute de regarder ce fichier.
const TERMINAISONS: &[&str] = &[
    ".env.example",
    ".env.sample",
    ".env.template",
    ".env.dist",
    ".env.defaults",
    ".env.local.example",
];

pub fn detect(tree: &RepoTree, compose: Option<&ComposeFacts>) -> ConfigFacts {
    detect_avec_build(tree, compose, None)
}

pub fn detect_avec_build(
    tree: &RepoTree,
    compose: Option<&ComposeFacts>,
    build: Option<&BuildRecipe>,
) -> ConfigFacts {
    let mut facts = ConfigFacts::default();

    if let Some((path, content)) = exemple(tree) {
        facts.variables = parse_dotenv(content, &path);
        facts.example_file = Some(path);
    }

    // Le compose complete : il porte souvent les variables reellement
    // necessaires au demarrage, la ou le .env.example est incomplet.
    if let Some(c) = compose {
        let provenance = c.path.clone();
        for svc in c.services.iter().filter(|s| s.is_app) {
            for (name, value) in &svc.environment {
                ajouter(&mut facts, name, non_empty(value), &provenance);
            }
        }
    }

    // Les `ENV` de l'etape finale du Dockerfile sont ce que l'image fixe
    // reellement au demarrage. Source moins riche que le compose — pas de
    // commentaires — mais parfois la seule.
    if let Some(b) = build {
        let provenance = b.dockerfile_path.clone();
        for (name, value) in &b.env {
            ajouter(&mut facts, name, non_empty(value), &provenance);
        }
    }

    facts
}

/// Le fichier d'exemple : chemin connu d'abord, puis terminaison reconnue.
///
/// A terminaison egale, le chemin le plus court gagne : `app.env.example` a la
/// racine est plus representatif que `tests/fixtures/x.env.example`.
fn exemple(tree: &RepoTree) -> Option<(String, &str)> {
    if let Some((p, c)) = tree.first_text(CANDIDATES) {
        return Some((p, c));
    }
    let mut trouves: Vec<String> = tree
        .paths()
        .into_iter()
        .filter(|p| {
            let bas = p.to_lowercase();
            TERMINAISONS.iter().any(|t| bas.ends_with(t))
        })
        .collect();
    trouves.sort_by_key(|p| (p.matches('/').count(), p.len(), p.clone()));
    let p = trouves.into_iter().next()?;
    let c = tree.text(&p)?;
    Some((p, c))
}

/// Ajoute une variable si aucune source plus sure ne l'a deja nommee.
fn ajouter(facts: &mut ConfigFacts, name: &str, valeur: Option<String>, source: &str) {
    if facts.variables.iter().any(|v| v.name == name) {
        return;
    }
    facts.variables.push(make_var(name, valeur, source));
}

/// Analyse un fichier au format `.env`.
///
/// Les lignes commentees sont lues elles aussi : un `# PORT=3000` documente la
/// valeur par defaut attendue, information qu'il serait dommage de perdre.
/// Elles ne sont retenues que si la variable n'est pas deja definie.
fn parse_dotenv(content: &str, source: &str) -> Vec<ConfigVar> {
    let mut out: Vec<ConfigVar> = Vec::new();
    let mut commented: Vec<ConfigVar> = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        let (is_comment, body) = match trimmed.strip_prefix('#') {
            Some(rest) => (true, rest.trim()),
            None => (false, trimmed),
        };

        let Some((name, value)) = body.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_start_matches("export ").trim();
        if name.is_empty() || !is_env_name(name) {
            continue;
        }

        let var = make_var(name, non_empty(value.trim()), source);
        if is_comment {
            commented.push(var);
        } else if !out.iter().any(|v| v.name == var.name) {
            out.push(var);
        }
    }

    for var in commented {
        if !out.iter().any(|v| v.name == var.name) {
            out.push(var);
        }
    }
    out
}

fn make_var(name: &str, default: Option<String>, source: &str) -> ConfigVar {
    let k = knowledge::get();
    ConfigVar {
        role: k.role_of(name),
        secret: k.is_secret(name),
        // Ne jamais reprendre la valeur d'un secret : celle d'un fichier
        // d'exemple est publique par construction.
        default: if k.is_secret(name) { None } else { default },
        name: name.to_string(),
        source: source.to_string(),
    }
}

fn non_empty(v: &str) -> Option<String> {
    let v = v.trim().trim_matches(|c| c == '"' || c == '\'').trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Un nom de variable d'environnement : majuscules, chiffres, tirets bas.
/// Ce filtre ecarte les fragments de phrase d'un commentaire en prose.
fn is_env_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && s.chars().any(|c| c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::facts::ConfigRole;

    const ENV: &str = r#"
# Configuration de l'application
PORT=3000
DATABASE_URL=postgresql://user:pass@localhost:5432/app
NEXTAUTH_SECRET=changeme-please
export LOG_LEVEL=info

# Optionnel : decommenter pour activer
# SMTP_HOST=smtp.example.com

ceci est une phrase = pas une variable
"#;

    fn vars(tree: &RepoTree) -> Vec<ConfigVar> {
        detect(tree, None).variables
    }

    fn tree() -> RepoTree {
        RepoTree::from_pairs([(".env.example", ENV)])
    }

    #[test]
    fn les_variables_sont_lues_avec_leur_role() {
        let v = vars(&tree());
        let by = |n: &str| v.iter().find(|x| x.name == n).cloned().unwrap();

        assert_eq!(by("PORT").role, ConfigRole::Port);
        assert_eq!(by("PORT").default.as_deref(), Some("3000"));
        assert_eq!(by("DATABASE_URL").role, ConfigRole::DatabaseUrl);
        assert_eq!(by("LOG_LEVEL").role, ConfigRole::LogLevel);
    }

    #[test]
    fn la_valeur_d_un_secret_n_est_jamais_reprise() {
        // Celle d'un fichier d'exemple est publique par construction.
        let v = vars(&tree());
        let secret = v.iter().find(|x| x.name == "NEXTAUTH_SECRET").unwrap();
        assert!(secret.secret);
        assert_eq!(
            secret.default, None,
            "la valeur d'exemple ne doit pas fuiter"
        );
    }

    #[test]
    fn une_variable_commentee_documente_quand_meme_un_defaut() {
        let v = vars(&tree());
        assert!(v.iter().any(|x| x.name == "SMTP_HOST"));
    }

    #[test]
    fn la_prose_d_un_commentaire_n_est_pas_prise_pour_une_variable() {
        let v = vars(&tree());
        assert!(
            v.iter().all(|x| is_env_name(&x.name)),
            "variables retenues : {:?}",
            v.iter().map(|x| &x.name).collect::<Vec<_>>()
        );
        assert!(!v.iter().any(|x| x.name.contains("phrase")));
    }

    #[test]
    fn le_prefixe_export_est_retire() {
        let v = vars(&tree());
        assert!(v.iter().any(|x| x.name == "LOG_LEVEL"));
        assert!(!v.iter().any(|x| x.name.contains("export")));
    }

    #[test]
    fn le_compose_complete_les_variables_absentes_du_fichier_d_exemple() {
        let compose_yml = r#"
services:
  app:
    build: .
    environment:
      - PORT=8080
      - EXTRA_SETTING=oui
"#;
        let compose = ynp_dockerfile::parse_compose("c.yml", compose_yml, None).unwrap();
        let facts = detect(&tree(), Some(&compose));

        // PORT existe deja dans le .env.example : sa valeur ne doit pas etre ecrasee.
        let port = facts.variables.iter().find(|v| v.name == "PORT").unwrap();
        assert_eq!(port.default.as_deref(), Some("3000"));
        assert!(facts.variables.iter().any(|v| v.name == "EXTRA_SETTING"));
    }

    #[test]
    fn un_depot_sans_fichier_d_exemple_ne_produit_rien() {
        let f = detect(&RepoTree::from_pairs([("README.md", "x")]), None);
        assert!(f.example_file.is_none());
        assert!(f.variables.is_empty());
    }
}
