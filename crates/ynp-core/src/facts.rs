//! `RepoFacts` : ce qui EST dans le depot, sans interpretation.
//!
//! Regle de separation stricte du pipeline : ce module ne contient que de la
//! collecte. Aucune decision de packaging ne s'y prend — elles sont toutes dans
//! [`AppSpec`](crate::spec::AppSpec). Cette frontiere est ce qui rend les
//! detecteurs testables un par un et parallelisables entre agents.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub const FACTS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoFacts {
    pub schema_version: u32,
    pub source: SourceRef,
    pub meta: RepoMeta,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub releases: Vec<Release>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Chemins relatifs du depot. Sert aux detecteurs « ce fichier existe-t-il ».
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree: Vec<String>,
    /// Recette extraite du Dockerfile. Le signal le plus riche du projet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildRecipe>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compose: Option<ComposeFacts>,
    pub stack: StackFacts,
    pub services: ServiceFacts,
    pub config: ConfigFacts,
    /// Source retenue, renseignee par `ynp-forge` apres telechargement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<SourceSelection>,
}

impl RepoFacts {
    pub fn new(source: SourceRef) -> Self {
        Self {
            schema_version: FACTS_SCHEMA_VERSION,
            source,
            ..Default::default()
        }
    }

    pub fn has_file(&self, path: &str) -> bool {
        self.tree.iter().any(|p| p == path)
    }

    pub fn has_any(&self, paths: &[&str]) -> bool {
        paths.iter().any(|p| self.has_file(p))
    }

    /// Premier fichier existant parmi les candidats, dans l'ordre de preference.
    pub fn first_of(&self, paths: &[&str]) -> Option<String> {
        paths
            .iter()
            .find(|p| self.has_file(p))
            .map(|p| p.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub forge: Forge,
    pub owner: String,
    pub repo: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Forge {
    #[default]
    GitHub,
    GitLab,
    Gitea,
    Forgejo,
}

impl Forge {
    /// Suffixe de strategie d'autoupdate attendu par l'outillage YunoHost,
    /// cf. `autoupdate.strategy = "latest_<forge>_release"`.
    pub fn autoupdate_slug(self) -> &'static str {
        match self {
            Forge::GitHub => "github",
            Forge::GitLab => "gitlab",
            Forge::Gitea => "gitea",
            Forge::Forgejo => "forgejo",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<String>,
    pub stars: u32,
    pub archived: bool,
    /// Date ISO-8601 du dernier push, pour la regle d'abandon MAINT001.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed_at: Option<String>,
    /// Identifiant SPDX tel que rendu par la forge, ex. `AGPL-3.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license_spdx: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub prerelease: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<ReleaseAsset>,
    /// Tarball genere par la forge. Toujours disponible, meme sans asset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tarball_url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// Ce qu'un Dockerfile dit du build, transpose en donnees.
///
/// C'est la piece qui remplace le LLM : un Dockerfile est deja une
/// specification de construction lisible par une machine.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRecipe {
    pub dockerfile_path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<BuildStage>,
    /// Paquets Debian, extraits litteralement des `apt-get install`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apt_packages: Vec<String>,
    /// Paquets Alpine, a traduire via assets/knowledge/apk-to-deb.toml.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apk_packages: Vec<String>,
    /// Lignes RUN utiles au build, hors installation de paquets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_steps: Vec<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expose: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmd: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workdir: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

impl BuildRecipe {
    /// Image de la derniere etape : celle qui porte le runtime, pas le builder.
    pub fn runtime_stage(&self) -> Option<&BuildStage> {
        self.stages.last()
    }

    /// Commande de demarrage, ENTRYPOINT et CMD concatenes comme le ferait Docker.
    pub fn start_command(&self) -> Option<String> {
        match (&self.entrypoint, &self.cmd) {
            (Some(e), Some(c)) if !e.is_empty() => Some(format!("{} {}", e.join(" "), c.join(" "))),
            (Some(e), None) if !e.is_empty() => Some(e.join(" ")),
            (_, Some(c)) if !c.is_empty() => Some(c.join(" ")),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildStage {
    pub image: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeFacts {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<ComposeService>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeService {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Vrai si le service est construit depuis le depot : c'est l'app elle-meme,
    /// par opposition aux services d'infrastructure (base, cache).
    pub is_app: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<u16>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub environment: IndexMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Technology {
    Php,
    #[serde(rename = "nodejs")]
    NodeJs,
    Python,
    Go,
    Ruby,
    Rust,
    Java,
    /// Fichiers statiques servis directement par nginx.
    Static,
    #[default]
    Unknown,
}

impl Technology {
    /// La resource `[resources.X]` correspondante du manifest, si elle existe.
    /// Python n'en a pas : c'est tout l'objet de la regle PY001.
    pub fn manifest_resource(self) -> Option<&'static str> {
        match self {
            Technology::NodeJs => Some("nodejs"),
            Technology::Ruby => Some("ruby"),
            Technology::Go => Some("go"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackFacts {
    pub primary: Technology,
    /// Version exigee par l'upstream, ex. `20` pour Node, `8.2` pour PHP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub package_managers: Vec<String>,
    pub has_lockfile: bool,
    /// Commande de build declaree par l'upstream (`scripts.build`, cible Makefile).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_script: Option<String>,
    /// Dependances npm/pip exigeant des paquets `-dev` cote systeme.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub native_deps: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Database {
    #[default]
    None,
    #[serde(rename = "mysql")]
    MySql,
    #[serde(rename = "postgresql")]
    PostgreSql,
    /// Fichier local : rien a provisionner cote YunoHost.
    Sqlite,
    #[serde(rename = "mongodb")]
    MongoDb,
    /// Detectee mais hors du perimetre YunoHost (Elasticsearch, ClickHouse...).
    Unsupported,
}

impl Database {
    /// Valeur de `[resources.database] type`, si la base est provisionnable
    /// par le coeur de YunoHost.
    pub fn manifest_type(self) -> Option<&'static str> {
        match self {
            Database::MySql => Some("mysql"),
            Database::PostgreSql => Some("postgresql"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceFacts {
    pub database: Database,
    /// Nom du service tel que vu dans compose, pour la tracabilite du constat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_evidence: Option<String>,
    pub needs_redis: bool,
    /// Services detectes sans equivalent YunoHost. Alimente la regle DB002.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
    /// Le runtime lui-meme exige docker-compose : blocage RUN001.
    pub requires_container_runtime: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFacts {
    /// Fichier d'exemple trouve (`.env.example`, `config.sample.yml`...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example_file: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variables: Vec<ConfigVar>,
}

impl ConfigFacts {
    pub fn get(&self, role: ConfigRole) -> Option<&ConfigVar> {
        self.variables.iter().find(|v| v.role == role)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigVar {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Role deduit via assets/knowledge/env-vars.toml.
    pub role: ConfigRole,
    /// Valeur a generer aleatoirement plutot qu'a recopier.
    pub secret: bool,
}

/// Role canonique d'une variable de configuration.
///
/// Reconnaitre `PORT`, `DATABASE_URL` ou `APP_URL` est ce qui permet de cabler
/// la configuration de l'app sur les valeurs YunoHost sans rien deviner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigRole {
    Port,
    DatabaseUrl,
    DatabaseHost,
    DatabaseName,
    DatabaseUser,
    DatabasePassword,
    BaseUrl,
    Secret,
    AdminEmail,
    AdminUser,
    AdminPassword,
    DataPath,
    SmtpHost,
    LogLevel,
    #[default]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_commande_de_demarrage_combine_entrypoint_et_cmd() {
        let mut r = BuildRecipe {
            entrypoint: Some(vec!["node".into()]),
            cmd: Some(vec!["server.js".into()]),
            ..Default::default()
        };
        assert_eq!(r.start_command().as_deref(), Some("node server.js"));

        r.entrypoint = None;
        assert_eq!(r.start_command().as_deref(), Some("server.js"));

        r.cmd = None;
        assert_eq!(r.start_command(), None);
    }

    #[test]
    fn l_etape_de_runtime_est_la_derniere_pas_le_builder() {
        let r = BuildRecipe {
            stages: vec![
                BuildStage {
                    image: "node".into(),
                    tag: Some("20".into()),
                    alias: Some("builder".into()),
                },
                BuildStage {
                    image: "nginx".into(),
                    tag: Some("alpine".into()),
                    alias: None,
                },
            ],
            ..Default::default()
        };
        assert_eq!(r.runtime_stage().unwrap().image, "nginx");
    }

    #[test]
    fn python_n_a_pas_de_resource_manifest_contrairement_a_node() {
        assert_eq!(Technology::NodeJs.manifest_resource(), Some("nodejs"));
        assert_eq!(Technology::Python.manifest_resource(), None);
    }

    #[test]
    fn seules_mysql_et_postgres_sont_provisionnables_par_le_coeur() {
        assert_eq!(Database::MySql.manifest_type(), Some("mysql"));
        assert_eq!(Database::Sqlite.manifest_type(), None);
        assert_eq!(Database::Unsupported.manifest_type(), None);
    }

    #[test]
    fn les_faits_repondent_sur_la_presence_de_fichiers() {
        let mut f = RepoFacts::new(SourceRef::default());
        f.tree = vec!["Dockerfile".into(), "package.json".into()];
        assert!(f.has_file("Dockerfile"));
        assert!(f.has_any(&["absent", "package.json"]));
        assert_eq!(
            f.first_of(&["absent", "package.json", "Dockerfile"])
                .as_deref(),
            Some("package.json")
        );
    }
}

#[cfg(test)]
mod serde_names {
    use super::*;

    /// Ces valeurs se retrouvent telles quelles dans appspec.toml, relu par des
    /// humains : « nodejs » et « postgresql », pas « node_js » ni « postgre_sql ».
    #[test]
    fn les_noms_serialises_sont_ceux_du_monde_reel() {
        let names = |t: Technology| serde_json::to_string(&t).unwrap();
        assert_eq!(names(Technology::NodeJs), "\"nodejs\"");
        assert_eq!(names(Technology::Php), "\"php\"");

        let db = |d: Database| serde_json::to_string(&d).unwrap();
        assert_eq!(db(Database::PostgreSql), "\"postgresql\"");
        assert_eq!(db(Database::MySql), "\"mysql\"");
        assert_eq!(db(Database::MongoDb), "\"mongodb\"");
    }
}

/// La source retenue pour le packaging, et sa somme de controle.
///
/// Conservee dans les faits pour que le pipeline puisse reprendre a l'etage
/// suivant sans retelecharger — et sans risquer que l'amont ait change entre
/// deux telechargements, ce qui publierait un sha256 ne correspondant a rien.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSelection {
    pub reference: String,
    /// Valeur de `autoupdate.strategy` dans le manifest.
    pub strategy: String,
    pub version: Option<String>,
    /// `release`, `tag` ou `commit`.
    pub kind: String,
    /// Archive des sources.
    pub url: String,
    pub sha256: String,
    /// Binaires deja construits, par architecture.
    ///
    /// Quand l'amont en publie, les paquets YunoHost les preferent aux sources :
    /// `gotify_ynh`, `memos_ynh` et `miniflux_ynh` procedent tous ainsi, avec
    /// `ram.build = 50M`. Compiler sur la machine cible est le principal motif
    /// d'echec d'installation sur les petites instances.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prebuilt: Vec<ArchAsset>,
}

impl SourceSelection {
    /// Vrai si l'on peut se dispenser de construire sur la machine cible.
    pub fn evite_la_compilation(&self) -> bool {
        !self.prebuilt.is_empty()
    }
}

/// Un binaire publie pour une architecture donnee.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchAsset {
    /// Nomenclature `dpkg --print-architecture` : amd64, i386, armhf, arm64.
    pub arch: String,
    pub name: String,
    pub url: String,
    pub sha256: String,
    /// Motif reconnaissant cet asset d'une version a l'autre, pour
    /// `autoupdate.asset.<arch>`.
    pub pattern: String,
    /// Faux pour un binaire nu, que `ynh_setup_source` deplace au lieu de
    /// l'extraire. Beaucoup de projets Go publient ainsi : miniflux livre un
    /// `miniflux-linux-amd64` sans extension, et le paquet officiel le declare
    /// avec `extract = false` et `rename`.
    #[serde(default)]
    pub extract: bool,
}
