//! Le mecanisme qui rend l'absence de LLM tenable.
//!
//! Un packager deterministe rencontre forcement des champs qu'il ne peut pas
//! deduire du depot. La reponse du projet n'est pas de deviner, c'est de le
//! dire : tout champ non trivial d'un [`AppSpec`](crate::spec::AppSpec) est un
//! [`Known<T>`], qui vaut soit une valeur, soit un [`Unresolved`] portant la
//! raison et l'endroit ou chercher.
//!
//! En TOML, les deux formes se distinguent a l'oeil :
//!
//! ```toml
//! execstart = "/var/www/foo/bin/server"                      # resolu
//! execstart = { unknown = "aucun CMD dans le Dockerfile", \
//!               look_in = ["Procfile", "README.md"] }        # a completer
//! ```
//!
//! Un agent (ou un humain) remplace la seconde forme par la premiere, puis
//! relance `generate`. `verify` refuse tout paquet ou il subsiste un
//! `FIXME(ynopack)`, ce qui garantit qu'aucun paquet devine ne sort du pipeline.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Pourquoi un champ n'a pas pu etre determine, et ou regarder pour le combler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unresolved {
    /// Raison lisible. La cle `unknown` sert aussi de discriminant TOML.
    pub unknown: String,
    /// Fichiers ou sections a inspecter pour trancher. Jamais vide en pratique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub look_in: Vec<String>,
}

impl Unresolved {
    pub fn new(reason: impl Into<String>, look_in: &[&str]) -> Self {
        Self {
            unknown: reason.into(),
            look_in: look_in.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Une valeur connue, ou la trace explicite de son absence.
///
/// L'ordre des variantes compte : `serde(untagged)` essaie `Value` en premier,
/// si bien qu'une chaine TOML reste une chaine et qu'une table `{ unknown = ... }`
/// est reconnue comme non resolue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Known<T> {
    Value(T),
    Unresolved(Unresolved),
}

impl<T> Known<T> {
    /// Paire symetrique de [`Known::unresolved`].
    pub fn resolved(v: T) -> Self {
        Known::Value(v)
    }

    pub fn unresolved(reason: impl Into<String>, look_in: &[&str]) -> Self {
        Known::Unresolved(Unresolved::new(reason, look_in))
    }

    pub fn is_resolved(&self) -> bool {
        matches!(self, Known::Value(_))
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Known::Value(v) => Some(v),
            Known::Unresolved(_) => None,
        }
    }

    pub fn reason(&self) -> Option<&Unresolved> {
        match self {
            Known::Value(_) => None,
            Known::Unresolved(u) => Some(u),
        }
    }

    /// Valeur, ou repli deterministe. Utilise pour les champs ou un defaut
    /// mediocre vaut mieux qu'un blocage (typiquement le texte libre).
    pub fn or(&self, fallback: T) -> T
    where
        T: Clone,
    {
        self.value().cloned().unwrap_or(fallback)
    }

    /// Le marqueur depose dans le fichier genere quand le champ manque.
    /// `verify` echoue tant qu'il en reste un : c'est le garde-fou anti-bluff.
    pub fn fixme(&self, field: &str) -> Option<String> {
        self.reason().map(|u| {
            let ou = if u.look_in.is_empty() {
                String::new()
            } else {
                format!(" | chercher dans : {}", u.look_in.join(", "))
            };
            format!("FIXME(ynopack): {field} non determine — {}{ou}", u.unknown)
        })
    }
}

impl<T: fmt::Display> fmt::Display for Known<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Known::Value(v) => write!(f, "{v}"),
            Known::Unresolved(u) => write!(f, "<non determine : {}>", u.unknown),
        }
    }
}

impl<T> From<T> for Known<T> {
    fn from(v: T) -> Self {
        Known::Value(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct Holder {
        execstart: Known<String>,
        port: Known<u16>,
    }

    #[test]
    fn une_valeur_et_une_absence_coexistent_dans_le_meme_toml() {
        let toml_src = r#"
execstart = { unknown = "aucun CMD dans le Dockerfile", look_in = ["Procfile"] }
port = 3000
"#;
        let h: Holder = toml::from_str(toml_src).unwrap();
        assert!(!h.execstart.is_resolved());
        assert_eq!(h.port.value(), Some(&3000));
        assert_eq!(h.execstart.reason().unwrap().look_in, vec!["Procfile"]);
    }

    #[test]
    fn le_champ_resolu_par_un_agent_se_relit_comme_une_valeur() {
        let h: Holder = toml::from_str("execstart = \"/usr/bin/foo\"\nport = 8080\n").unwrap();
        assert_eq!(
            h.execstart.value().map(String::as_str),
            Some("/usr/bin/foo")
        );
    }

    #[test]
    fn aller_retour_toml_preserve_la_distinction() {
        let h = Holder {
            execstart: Known::unresolved("pas de CMD", &["Procfile", "README.md"]),
            port: Known::resolved(8080),
        };
        let s = toml::to_string(&h).unwrap();
        let back: Holder = toml::from_str(&s).unwrap();
        assert_eq!(h, back);
    }

    #[test]
    fn le_marqueur_fixme_nomme_le_champ_et_ou_chercher() {
        let k: Known<String> = Known::unresolved("pas de CMD", &["Procfile"]);
        let m = k.fixme("execstart").unwrap();
        assert!(m.starts_with("FIXME(ynopack): execstart"));
        assert!(m.contains("Procfile"));
        assert!(Known::resolved("x".to_string())
            .fixme("execstart")
            .is_none());
    }
}
