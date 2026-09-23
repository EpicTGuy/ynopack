//! Detecteurs : ce qui est dans le depot, sans interpretation.
//!
//! Regle de separation du pipeline : ce crate collecte, il ne decide pas. Un
//! detecteur qui ecrirait « puisque c'est du Node, le port est 3000 » violerait
//! cette frontiere — il constate `EXPOSE 3000`, et c'est `ynp-spec` qui en tire
//! une consequence.
//!
//! C'est ce qui rend chaque detecteur testable sur une arborescence fabriquee,
//! sans reseau, et donc parallelisable entre agents.

pub mod config;
pub mod health;
pub mod knowledge;
pub mod services;
pub mod stack;

use ynp_core::facts::{Release, RepoFacts, RepoMeta, SourceRef, FACTS_SCHEMA_VERSION};
use ynp_core::tree::RepoTree;

/// Ce que la forge a fourni, en amont de l'analyse du contenu.
///
/// Sortir ces donnees du detecteur permet de tester l'analyse sans reseau :
/// `ynp-forge` remplira cette structure, les tests la fabriquent.
#[derive(Debug, Clone, Default)]
pub struct ForgeData {
    pub source: SourceRef,
    pub meta: RepoMeta,
    pub releases: Vec<Release>,
    pub tags: Vec<String>,
}

/// Assemble les faits d'un depot.
///
/// L'ordre des appels n'est pas arbitraire : le Dockerfile et le compose
/// alimentent la detection de stack, qui alimente celle des services.
pub fn analyze(forge: ForgeData, tree: &RepoTree) -> RepoFacts {
    let mut facts = RepoFacts::new(forge.source.clone());
    facts.schema_version = FACTS_SCHEMA_VERSION;
    facts.meta = forge.meta;
    facts.releases = forge.releases;
    facts.tags = forge.tags;
    facts.tree = tree.paths();

    let dockerfile = ynp_dockerfile::find_dockerfile(&facts.tree);
    facts.build = dockerfile.as_ref().and_then(|path| {
        tree.text(path)
            .map(|content| ynp_dockerfile::parse_dockerfile(path, content))
    });

    let repo_hint = forge.source.repo.as_str();
    facts.compose = ynp_dockerfile::find_compose(&facts.tree).and_then(|path| {
        let content = tree.text(&path)?;
        ynp_dockerfile::parse_compose(&path, content, Some(repo_hint))
    });

    facts.stack = stack::detect(tree, facts.build.as_ref());
    facts.config = config::detect(tree, facts.compose.as_ref());
    facts.services = services::detect(
        tree,
        facts.compose.as_ref(),
        &facts.config,
        &facts.stack,
        dockerfile.is_some(),
    );

    facts
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::facts::{Database, Forge, Technology};

    fn forge_data(repo: &str) -> ForgeData {
        ForgeData {
            source: SourceRef {
                forge: Forge::GitHub,
                owner: "acme".into(),
                repo: repo.into(),
                url: format!("https://github.com/acme/{repo}"),
                default_branch: Some("main".into()),
                commit: None,
            },
            meta: RepoMeta {
                license_spdx: Some("AGPL-3.0".into()),
                pushed_at: Some("2026-09-01".into()),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// Une application Node avec base PostgreSQL, telle qu'on en rencontre.
    fn depot_realiste() -> RepoTree {
        RepoTree::from_pairs([
            (
                "Dockerfile",
                "FROM node:20-bookworm-slim\nWORKDIR /app\nRUN apt-get install -y libvips-dev \
                 && npm ci && npm run build\nEXPOSE 3000\nCMD [\"node\", \"dist/main.js\"]\n",
            ),
            (
                "docker-compose.yml",
                "services:\n  db:\n    image: postgres:16\n  app:\n    build: .\n    ports: [\"3000:3000\"]\n",
            ),
            (
                "package.json",
                r#"{"engines":{"node":">=20"},"scripts":{"build":"tsc"},"dependencies":{"sharp":"^0.33"}}"#,
            ),
            ("package-lock.json", "{}"),
            (".env.example", "PORT=3000\nDATABASE_URL=postgresql://u:p@db/app\nJWT_SECRET=changeme\n"),
        ])
    }

    #[test]
    fn un_depot_realiste_est_entierement_decode() {
        let f = analyze(forge_data("widget"), &depot_realiste());

        assert_eq!(f.stack.primary, Technology::NodeJs);
        assert_eq!(f.stack.runtime_version.as_deref(), Some("20"));
        assert_eq!(f.stack.native_deps, vec!["sharp"]);
        assert_eq!(f.services.database, Database::PostgreSql);

        let build = f.build.as_ref().unwrap();
        assert_eq!(build.expose, vec![3000]);
        assert_eq!(build.apt_packages, vec!["libvips-dev"]);
        assert_eq!(build.start_command().as_deref(), Some("node dist/main.js"));

        assert_eq!(f.config.variables.len(), 3);
        assert!(
            f.config
                .variables
                .iter()
                .find(|v| v.name == "JWT_SECRET")
                .unwrap()
                .secret
        );
        assert!(!f.services.requires_container_runtime);
    }

    #[test]
    fn le_nom_du_depot_aide_a_designer_le_service_applicatif() {
        let tree = RepoTree::from_pairs([(
            "docker-compose.yml",
            "services:\n  db:\n    image: postgres:16\n  widget:\n    image: ghcr.io/acme/widget\n",
        )]);
        let f = analyze(forge_data("widget"), &tree);

        let app: Vec<_> = f
            .compose
            .unwrap()
            .services
            .into_iter()
            .filter(|s| s.is_app)
            .collect();
        assert_eq!(app.len(), 1);
        assert_eq!(app[0].name, "widget");
    }

    #[test]
    fn un_depot_vide_produit_des_faits_vides_et_non_une_erreur() {
        let f = analyze(forge_data("rien"), &RepoTree::new());

        assert_eq!(f.stack.primary, Technology::Unknown);
        assert_eq!(f.services.database, Database::None);
        assert!(f.build.is_none());
        assert!(f.compose.is_none());
        assert!(f.tree.is_empty());
    }

    #[test]
    fn les_faits_font_un_aller_retour_json_sans_perte() {
        // `analyze` produit facts.json, relu par `assess` et `plan`.
        let f = analyze(forge_data("widget"), &depot_realiste());
        let text = serde_json::to_string(&f).unwrap();
        let back: RepoFacts = serde_json::from_str(&text).unwrap();
        assert_eq!(f, back);
    }
}
