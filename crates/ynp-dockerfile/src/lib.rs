//! Transpilation `Dockerfile` / `docker-compose.yml` vers des donnees exploitables.
//!
//! C'est le composant qui porte le pari zero-LLM du projet : plutot que de
//! demander a un modele de deviner comment une application se construit, on lit
//! la recette que l'upstream a deja ecrite pour Docker.
//!
//! ```no_run
//! # use ynp_dockerfile::parse_dockerfile;
//! let recipe = parse_dockerfile("Dockerfile", "FROM node:20\nEXPOSE 3000\nCMD [\"node\",\"s.js\"]");
//! assert_eq!(recipe.expose, vec![3000]);
//! assert_eq!(recipe.start_command().as_deref(), Some("node s.js"));
//! ```

pub mod compose;
pub mod dockerfile;
pub mod lexer;
pub mod shell;

pub use compose::{find_compose, parse as parse_compose, services_from_compose, COMPOSE_NAMES};
pub use dockerfile::{find_dockerfile, parse as parse_dockerfile, DOCKERFILE_NAMES};
