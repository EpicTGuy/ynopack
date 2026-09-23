//! Chargement des tables de correspondance.
//!
//! Elles sont incorporees au binaire : `ynopack` doit pouvoir tourner sur un
//! hote de test qui n'a recu que l'executable, sans arborescence de donnees a
//! cote. Les fichiers restent lisibles et modifiables dans `assets/knowledge/`.

use indexmap::IndexMap;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;
use ynp_core::facts::ConfigRole;

const ENV_VARS: &str = include_str!("../../../assets/knowledge/env-vars.toml");
const APK_TO_DEB: &str = include_str!("../../../assets/knowledge/apk-to-deb.toml");
const RUNTIME_VERSIONS: &str = include_str!("../../../assets/knowledge/runtime-versions.toml");
const UNSUPPORTED: &str = include_str!("../../../assets/knowledge/unsupported-services.toml");
const NPM_NATIVE: &str = include_str!("../../../assets/knowledge/npm-native-deps.toml");

#[derive(Debug, Deserialize)]
struct EnvVarTable {
    rule: Vec<EnvRule>,
}

#[derive(Debug, Deserialize)]
struct EnvRule {
    role: ConfigRole,
    #[serde(default)]
    exact: Vec<String>,
    #[serde(default)]
    suffix: Vec<String>,
    #[serde(default)]
    contains: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct MapTable<T> {
    packages: Option<IndexMap<String, T>>,
    deps: Option<IndexMap<String, T>>,
}

#[derive(Debug, Deserialize)]
struct RuntimeTable {
    bookworm: BTreeMap<String, String>,
    resources: ResourcesTable,
}

#[derive(Debug, Deserialize)]
struct ResourcesTable {
    provisionable: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UnsupportedTable {
    services: UnsupportedServices,
}

#[derive(Debug, Deserialize)]
struct UnsupportedServices {
    blocking: Vec<String>,
    replaced: Vec<String>,
}

/// Toutes les tables, chargees une seule fois.
pub struct Knowledge {
    env_rules: Vec<EnvRule>,
    apk_to_deb: IndexMap<String, String>,
    npm_native: IndexMap<String, Vec<String>>,
    bookworm: BTreeMap<String, String>,
    provisionable: Vec<String>,
    unsupported: Vec<String>,
    replaced: Vec<String>,
}

static KNOWLEDGE: OnceLock<Knowledge> = OnceLock::new();

pub fn get() -> &'static Knowledge {
    KNOWLEDGE.get_or_init(|| {
        // Un `expect` est justifie ici : les tables sont incorporees au binaire,
        // donc verifiees a la compilation par les tests. Une table invalide est
        // un bug du projet, pas une erreur d'execution a gerer.
        let env: EnvVarTable = toml::from_str(ENV_VARS).expect("env-vars.toml invalide");
        let apk: MapTable<String> = toml::from_str(APK_TO_DEB).expect("apk-to-deb.toml invalide");
        let npm: MapTable<Vec<String>> =
            toml::from_str(NPM_NATIVE).expect("npm-native-deps.toml invalide");
        let rt: RuntimeTable =
            toml::from_str(RUNTIME_VERSIONS).expect("runtime-versions.toml invalide");
        let un: UnsupportedTable =
            toml::from_str(UNSUPPORTED).expect("unsupported-services.toml invalide");

        Knowledge {
            env_rules: env.rule,
            apk_to_deb: apk.packages.unwrap_or_default(),
            npm_native: npm.deps.unwrap_or_default(),
            bookworm: rt.bookworm,
            provisionable: rt.resources.provisionable,
            unsupported: un.services.blocking,
            replaced: un.services.replaced,
        }
    })
}

impl Knowledge {
    /// Role d'une variable de configuration d'apres son nom.
    ///
    /// La premiere regle qui correspond gagne, d'ou l'ordre des entrees dans
    /// `env-vars.toml` : les roles precis avant les generaux.
    pub fn role_of(&self, name: &str) -> ConfigRole {
        let upper = name.to_ascii_uppercase();
        for rule in &self.env_rules {
            let hit = rule.exact.contains(&upper)
                || rule.suffix.iter().any(|s| upper.ends_with(s.as_str()))
                || rule.contains.iter().any(|c| upper.contains(c.as_str()));
            if hit {
                return rule.role;
            }
        }
        ConfigRole::Other
    }

    /// Vrai si la valeur doit etre regeneree plutot que recopiee : un secret
    /// present dans un fichier d'exemple est, par construction, public.
    pub fn is_secret(&self, name: &str) -> bool {
        matches!(
            self.role_of(name),
            ConfigRole::Secret | ConfigRole::DatabasePassword | ConfigRole::AdminPassword
        )
    }

    /// Traduit un paquet Alpine en paquet Debian.
    ///
    /// `Some("")` signale une correspondance connue mais sans objet en Debian
    /// (`musl-dev`), a distinguer de `None`, qui signale l'inconnu.
    pub fn apk_to_deb(&self, apk: &str) -> Option<&str> {
        self.apk_to_deb.get(apk).map(String::as_str)
    }

    /// Paquets Debian exiges par un module npm qui compile du code natif.
    pub fn npm_native_deps(&self, module: &str) -> &[String] {
        self.npm_native
            .get(module)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Version fournie nativement par bookworm, si la technologie y figure.
    pub fn bookworm_version(&self, tech: &str) -> Option<&str> {
        self.bookworm.get(tech).map(String::as_str)
    }

    /// Vrai si YunoHost sait provisionner une version arbitraire de ce runtime.
    /// Faux pour Python : c'est l'objet de la regle PY001.
    pub fn is_provisionable(&self, tech: &str) -> bool {
        self.provisionable.iter().any(|t| t == tech)
    }

    pub fn is_unsupported_service(&self, image: &str) -> bool {
        let lower = image.to_lowercase();
        self.unsupported.iter().any(|s| lower.contains(s.as_str()))
    }

    pub fn is_replaced_service(&self, image: &str) -> bool {
        let lower = image.to_lowercase();
        self.replaced.iter().any(|s| lower.contains(s.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toutes_les_tables_incorporees_sont_valides() {
        // Ce test est ce qui autorise les `expect` du chargeur.
        let k = get();
        assert!(!k.env_rules.is_empty());
        assert!(!k.apk_to_deb.is_empty());
        assert!(!k.npm_native.is_empty());
        assert!(!k.bookworm.is_empty());
    }

    #[test]
    fn les_roles_usuels_sont_reconnus() {
        let k = get();
        assert_eq!(k.role_of("DATABASE_URL"), ConfigRole::DatabaseUrl);
        assert_eq!(k.role_of("PORT"), ConfigRole::Port);
        assert_eq!(k.role_of("HTTP_PORT"), ConfigRole::Port);
        assert_eq!(k.role_of("NEXTAUTH_URL"), ConfigRole::BaseUrl);
        assert_eq!(k.role_of("POSTGRES_PASSWORD"), ConfigRole::DatabasePassword);
        assert_eq!(k.role_of("TOTALEMENT_INCONNU"), ConfigRole::Other);
    }

    #[test]
    fn une_regle_precise_l_emporte_sur_une_regle_generale() {
        // POSTGRES_PASSWORD contient « PASSWORD », qui declencherait la regle
        // generique des secrets ; la regle de mot de passe de base est declaree
        // avant et doit gagner.
        let k = get();
        assert_eq!(k.role_of("POSTGRES_PASSWORD"), ConfigRole::DatabasePassword);
        assert_eq!(k.role_of("JWT_SECRET"), ConfigRole::Secret);
    }

    #[test]
    fn un_suffixe_rattrape_les_variantes_de_port() {
        let k = get();
        assert_eq!(k.role_of("GRIST_PORT"), ConfigRole::Port);
        assert_eq!(k.role_of("METRICS_PORT"), ConfigRole::Port);
    }

    #[test]
    fn les_valeurs_a_regenerer_sont_identifiees() {
        let k = get();
        assert!(k.is_secret("SECRET_KEY"));
        assert!(k.is_secret("DB_PASSWORD"));
        assert!(!k.is_secret("PORT"));
        assert!(!k.is_secret("LOG_LEVEL"));
    }

    #[test]
    fn la_traduction_alpine_distingue_l_inconnu_du_sans_objet() {
        let k = get();
        assert_eq!(k.apk_to_deb("build-base"), Some("build-essential"));
        assert_eq!(k.apk_to_deb("vips-dev"), Some("libvips-dev"));
        // Connu, mais sans equivalent Debian.
        assert_eq!(k.apk_to_deb("musl-dev"), Some(""));
        // Inconnu : a signaler, pas a inventer.
        assert_eq!(k.apk_to_deb("paquet-jamais-vu"), None);
    }

    #[test]
    fn python_n_est_pas_provisionnable_contrairement_a_node() {
        let k = get();
        assert!(k.is_provisionable("nodejs"));
        assert!(k.is_provisionable("go"));
        assert!(
            !k.is_provisionable("python"),
            "c'est tout l'objet de la regle PY001"
        );
        assert_eq!(k.bookworm_version("python"), Some("3.11"));
    }

    #[test]
    fn les_modules_npm_natifs_portent_leurs_dependances_cachees() {
        let k = get();
        assert_eq!(k.npm_native_deps("sharp"), ["libvips-dev"]);
        assert!(k.npm_native_deps("canvas").len() > 3);
        assert!(k.npm_native_deps("lodash").is_empty());
    }

    #[test]
    fn un_service_remplace_par_yunohost_n_est_pas_un_blocage() {
        let k = get();
        assert!(k.is_unsupported_service("docker.elastic.co/elasticsearch:8"));
        assert!(!k.is_unsupported_service("nginx:alpine"));
        assert!(k.is_replaced_service("nginx:alpine"));
    }
}
