//! `AppSpec` : le seul endroit ou une decision de packaging est prise.
//!
//! En amont, `analyze` collecte des faits ; en aval, `generate` rend des
//! templates sans rien decider. Entre les deux, ce fichier TOML concentre les
//! arbitrages — et c'est donc le seul que relise un humain ou un agent.
//!
//! Corollaire volontaire : **un agent n'ecrit jamais de bash**. Il edite
//! `appspec.toml`, relance `generate` puis `verify`. Tout ce qui n'a pas pu
//! etre deduit apparait comme un [`Known::Unresolved`], donc comme une question
//! explicite plutot qu'une invention.

use crate::facts::{Database, Technology};
use crate::known::Known;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

pub const SPEC_SCHEMA_VERSION: u32 = 1;

/// Version de YunoHost visee par defaut. Alignee sur l'instance de test (12.1.26)
/// et sur ce qu'exige `example_ynh` en amont.
pub const DEFAULT_YUNOHOST_MIN: &str = ">= 12.1.17";
/// Jeu de helpers cible. La 2.1 a renomme la plupart des helpers : generer les
/// anciens noms declenche une erreur du linter officiel.
pub const DEFAULT_HELPERS_VERSION: &str = "2.1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppSpec {
    pub schema_version: u32,
    pub app: AppIdentity,
    pub upstream: Upstream,
    pub integration: Integration,
    pub install: InstallQuestions,
    pub resources: Resources,
    pub runtime: Runtime,
    pub features: Features,
    #[serde(default)]
    pub docs: Docs,
}

impl AppSpec {
    /// Tous les champs non resolus, sous la forme `(chemin, marqueur FIXME)`.
    ///
    /// C'est ce que `assess` affiche a l'utilisateur et ce que `verify`
    /// transforme en echec de la gate G2.
    pub fn unresolved(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut push = |path: &str, m: Option<String>| {
            if let Some(m) = m {
                out.push((path.to_string(), m));
            }
        };
        push(
            "app.description_en",
            self.app.description_en.fixme("app.description_en"),
        );
        push("app.version", self.app.version.fixme("app.version"));
        push(
            "upstream.license",
            self.upstream.license.fixme("upstream.license"),
        );
        push(
            "resources.sources.url",
            self.resources.sources.url.fixme("resources.sources.url"),
        );
        push(
            "resources.sources.sha256",
            self.resources
                .sources
                .sha256
                .fixme("resources.sources.sha256"),
        );
        push(
            "runtime.technology",
            self.runtime.technology.fixme("runtime.technology"),
        );
        if self.features.systemd {
            push(
                "runtime.execstart",
                self.runtime.execstart.fixme("runtime.execstart"),
            );
        }
        out
    }

    pub fn is_complete(&self) -> bool {
        self.unresolved().is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppIdentity {
    /// Minuscules, chiffres et tirets. Sert aussi de nom d'utilisateur systeme,
    /// de nom de dossier et de prefixe de conf nginx.
    pub id: String,
    /// Nom affiche. Le linter officiel refuse au-dela de 23 caracteres.
    pub name: String,
    /// 150 caracteres maximum, affichee dans le catalogue.
    pub description_en: Known<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_fr: Option<String>,
    /// Version amont seule : le suffixe `~ynhN` est ajoute a la generation.
    pub version: Known<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub maintainers: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    /// Identifiant SPDX. Seul champ obligatoire de la section cote YunoHost.
    pub license: Known<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub demo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admindoc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub userdoc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Le defaut d'un champ textuel est *non resolu*, jamais la chaine vide.
/// Oublier de renseigner un champ produit ainsi un FIXME visible plutot qu'un
/// paquet silencieusement incomplet.
impl Default for Known<String> {
    fn default() -> Self {
        Known::unresolved("non renseigne", &[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integration {
    pub yunohost_min: String,
    pub helpers_version: String,
    /// `"all"` ou une liste en nomenclature `dpkg --print-architecture`.
    pub architectures: Architectures,
    pub multi_instance: bool,
    pub ldap: Triple,
    pub sso: Triple,
    pub disk: String,
    pub ram_build: String,
    pub ram_runtime: String,
}

impl Default for Integration {
    fn default() -> Self {
        Self {
            yunohost_min: DEFAULT_YUNOHOST_MIN.to_string(),
            helpers_version: DEFAULT_HELPERS_VERSION.to_string(),
            architectures: Architectures::All,
            multi_instance: false,
            ldap: Triple::NotRelevant,
            sso: Triple::NotRelevant,
            disk: "50M".into(),
            ram_build: "50M".into(),
            ram_runtime: "50M".into(),
        }
    }
}

/// `architectures = "all"` ou une liste. Les deux formes sont valides dans le
/// manifest, on les represente sans forcer l'une ou l'autre.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Architectures {
    All,
    Only(Vec<String>),
}

impl serde::Serialize for Architectures {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Architectures::All => s.serialize_str("all"),
            Architectures::Only(v) => v.serialize(s),
        }
    }
}

impl<'de> serde::Deserialize<'de> for Architectures {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            One(String),
            Many(Vec<String>),
        }
        Ok(match Raw::deserialize(d)? {
            Raw::One(s) if s == "all" => Architectures::All,
            Raw::One(s) => Architectures::Only(vec![s]),
            Raw::Many(v) => Architectures::Only(v),
        })
    }
}

/// `true` / `false` / `"not_relevant"`, comme l'exige le manifest pour `ldap` et `sso`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Triple {
    Yes,
    No,
    /// L'app n'a pas de notion de compte utilisateur.
    NotRelevant,
}

impl Triple {
    pub fn manifest_value(self) -> &'static str {
        match self {
            Triple::Yes => "true",
            Triple::No => "false",
            Triple::NotRelevant => "\"not_relevant\"",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallQuestions {
    pub url_scheme: UrlScheme,
    /// Groupe autorise a l'installation : `visitors`, `all_users`, ...
    pub init_main_permission: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<Question>,
}

impl Default for InstallQuestions {
    fn default() -> Self {
        Self {
            url_scheme: UrlScheme::DomainAndPath,
            init_main_permission: "visitors".into(),
            extra: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UrlScheme {
    /// `domain.tld/app` : question `domain` + question `path`.
    DomainAndPath,
    /// L'app exige un domaine dedie : question `domain` seule.
    FullDomain,
    /// Pas de composante web (daemon pur).
    NoUrl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub name: String,
    /// Parmi : string, text, select, boolean, password, email, url, number, user, group, ...
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask_en: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default)]
    pub optional: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    pub sources: Sources,
    pub system_user: bool,
    pub install_dir: bool,
    pub data_dir: bool,
    /// Chemin expose par la permission principale, typiquement `/`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_permission_url: Option<String>,
    /// Reserve un port pour le reverse-proxy interne nginx -> app.
    pub ports: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apt_packages: Vec<String>,
    pub database: Database,
    /// Versions de runtime a provisionner par le coeur (`[resources.nodejs]`...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nodejs_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruby_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub composer_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sources {
    pub url: Known<String>,
    pub sha256: Known<String>,
    /// `latest_github_release`, `latest_github_tag`, `latest_github_commit`...
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoupdate_strategy: Option<String>,
    /// `false` quand l'archive n'a pas de repertoire intermediaire.
    #[serde(default = "default_true")]
    pub in_subdir: bool,
}

fn default_true() -> bool {
    true
}

/// Les decisions qui n'ont pas d'equivalent direct dans le manifest et qui
/// pilotent le contenu des scripts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runtime {
    pub technology: Known<Technology>,
    /// Commandes de build, dans l'ordre, executees dans `$install_dir`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_steps: Vec<String>,
    /// Valeur de `ExecStart=` de l'unite systemd.
    pub execstart: Known<String>,
    /// Variable par laquelle l'app apprend son port. Cable sur `$port`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_env_var: Option<String>,
    /// Fichier de conf a generer dans `$install_dir`, avec ses substitutions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_file: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub env: IndexMap<String, String>,
}

impl Default for Known<Technology> {
    fn default() -> Self {
        Known::unresolved("stack non identifiee", &["Dockerfile", "README.md"])
    }
}

/// Briques d'integration a cabler. Chacune conditionne des blocs dans plusieurs
/// scripts a la fois : activer `systemd` ajoute du code dans install, remove,
/// upgrade, backup, restore et change_url.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Features {
    pub nginx: bool,
    pub systemd: bool,
    pub phpfpm: bool,
    pub logrotate: bool,
    pub fail2ban: bool,
    pub cron: bool,
    pub change_url: bool,
    /// Declare le service aupres de YunoHost (`yunohost service add`).
    pub service_integration: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Docs {
    /// Corps de `doc/DESCRIPTION.md`. Repli deterministe : description de la
    /// forge, puis premier paragraphe du README amont.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_install: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_install: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admin: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> AppSpec {
        AppSpec {
            schema_version: SPEC_SCHEMA_VERSION,
            app: AppIdentity {
                id: "foo".into(),
                name: "Foo".into(),
                description_en: Known::resolved("A thing".into()),
                description_fr: None,
                version: Known::resolved("1.2.3".into()),
                maintainers: vec![],
            },
            upstream: Upstream {
                license: Known::resolved("AGPL-3.0".into()),
                ..Default::default()
            },
            integration: Integration::default(),
            install: InstallQuestions::default(),
            resources: Resources {
                sources: Sources {
                    url: Known::resolved("https://x/v1.tar.gz".into()),
                    sha256: Known::resolved("ab".repeat(32)),
                    autoupdate_strategy: None,
                    in_subdir: true,
                },
                ..Default::default()
            },
            runtime: Runtime {
                technology: Known::resolved(Technology::NodeJs),
                execstart: Known::resolved("/var/www/foo/bin/s".into()),
                ..Default::default()
            },
            features: Features {
                systemd: true,
                nginx: true,
                ..Default::default()
            },
            docs: Docs::default(),
        }
    }

    #[test]
    fn une_spec_complete_ne_laisse_aucun_fixme() {
        let s = minimal();
        assert!(s.is_complete(), "{:?}", s.unresolved());
    }

    #[test]
    fn un_champ_non_resolu_remonte_avec_son_chemin() {
        let mut s = minimal();
        s.runtime.execstart = Known::unresolved("aucun CMD", &["Procfile"]);
        let u = s.unresolved();
        assert_eq!(u.len(), 1);
        assert_eq!(u[0].0, "runtime.execstart");
        assert!(u[0].1.contains("FIXME(ynopack)"));
    }

    #[test]
    fn execstart_n_est_exige_que_si_l_app_a_un_service_systemd() {
        let mut s = minimal();
        s.runtime.execstart = Known::unresolved("aucun CMD", &[]);
        s.features.systemd = false;
        assert!(
            s.is_complete(),
            "une app sans daemon n'a pas besoin d'ExecStart"
        );
    }

    #[test]
    fn la_spec_fait_un_aller_retour_toml_sans_perte() {
        let s = minimal();
        let text = toml::to_string_pretty(&s).unwrap();
        let back: AppSpec = toml::from_str(&text).unwrap();
        assert_eq!(s, back);
    }
}

/// L'exemple documentaire doit rester valide : s'il ne se relit plus, c'est que
/// le format a bouge sans que la doc suive.
#[cfg(test)]
mod exemple_documente {
    use super::*;

    #[test]
    fn l_exemple_de_la_doc_se_relit_et_est_complet() {
        let src = include_str!("../../../tests/fixtures/appspec.example.toml");
        let spec: AppSpec = toml::from_str(src).expect("appspec.example.toml doit rester valide");

        assert_eq!(spec.app.id, "grist");
        assert_eq!(spec.runtime.technology.value(), Some(&Technology::NodeJs));
        assert_eq!(spec.resources.database, Database::PostgreSql);
        assert!(
            spec.is_complete(),
            "l'exemple ne doit contenir aucun FIXME : {:?}",
            spec.unresolved()
        );
    }
}
