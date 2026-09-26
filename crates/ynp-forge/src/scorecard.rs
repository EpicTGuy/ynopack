//! Notes OpenSSF Scorecard.
//!
//! La fondation OpenSSF passe en revue des centaines de milliers de depots et
//! publie, librement et sans authentification, une note par critere : le
//! projet est-il entretenu, les changements sont-ils relus, les versions
//! sont-elles signees. C'est la seule source publique et reguliere sur la
//! maniere dont un projet est tenu.
//!
//! Elle ne couvre que GitHub, et seulement les depots deja passes dans sa
//! moulinette : l'absence de note ne dit rien du projet.

use serde::{Deserialize, Serialize};

const API: &str = "https://api.securityscorecards.dev/projects";

/// Les criteres qu'on retient, et ce qu'ils veulent dire en clair.
///
/// Scorecard en publie une vingtaine, dont beaucoup ne concernent que la
/// chaine de construction. Ceux-la parlent de la conduite du projet, qui est
/// ce qu'on cherche a savoir avant de packager.
const RETENUS: &[(&str, &str)] = &[
    ("Maintained", "entretenu"),
    ("Code-Review", "changements relus"),
    ("Security-Policy", "politique de securite"),
    ("Signed-Releases", "versions signees"),
    ("Vulnerabilities", "failles connues"),
    ("License", "licence declaree"),
];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scorecard {
    /// Note d'ensemble, en dixiemes : 67 pour 6,7 sur 10.
    ///
    /// Un entier plutot qu'un flottant : une note sur dix a un chiffre apres
    /// la virgule n'est pas une mesure continue, et la comparer exactement
    /// n'aurait pas de sens en flottant.
    pub dixiemes: u16,
    /// Date de la derniere revue, au format `AAAA-MM-JJ`.
    pub date: String,
    pub criteres: Vec<Critere>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Critere {
    /// Libelle en clair, pas le nom technique du critere.
    pub nom: String,
    /// 0 a 10. Scorecard rend -1 quand il n'a pas pu conclure ; ces
    /// criteres-la sont ecartes plutot que presentes comme un zero.
    pub note: u8,
}

/// Interroge Scorecard pour un depot GitHub.
///
/// Rend `None` sans bruit quand il n'y a pas de note : le service ne couvre
/// que GitHub, et seulement une partie de ses depots. Une absence n'est pas
/// une erreur, et surtout pas un mauvais signe.
pub async fn pour(depot: &str) -> Option<Scorecard> {
    if depot.split('/').count() != 2 {
        return None;
    }
    let client = reqwest::Client::builder()
        .user_agent(concat!("yunopack/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let r = client
        .get(format!("{API}/github.com/{depot}"))
        .send()
        .await
        .ok()?;
    if !r.status().is_success() {
        return None;
    }
    lire(&r.text().await.ok()?)
}

/// Extrait la note d'une reponse. Separe de l'appel reseau pour etre testable.
pub fn lire(json: &str) -> Option<Scorecard> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    // Arrondi des la lecture, en dixiemes : une note sur dix a un chiffre
    // apres la virgule, et `6.7` en flottant vaut `6.699999809265137`.
    let dixiemes = (v.get("score")?.as_f64()? * 10.0).round().clamp(0.0, 100.0) as u16;

    let criteres = v
        .get("checks")
        .and_then(|c| c.as_array())
        .map(|checks| {
            RETENUS
                .iter()
                .filter_map(|(technique, clair)| {
                    let c = checks
                        .iter()
                        .find(|c| c.get("name").and_then(|n| n.as_str()) == Some(technique))?;
                    let n = c.get("score")?.as_i64()?;
                    // -1 signifie « n'a pas pu conclure » ; l'afficher comme un
                    // zero accuserait le projet a tort.
                    (n >= 0).then_some(Critere {
                        nom: clair.to_string(),
                        note: n.min(10) as u8,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(Scorecard {
        dixiemes,
        date: v
            .get("date")
            .and_then(|d| d.as_str())
            .unwrap_or_default()
            .to_string(),
        criteres,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extrait reel de la reponse pour miniflux/v2.
    const REPONSE: &str = r#"{
      "date": "2026-09-21",
      "score": 6.7,
      "checks": [
        {"name": "Security-Policy", "score": 10},
        {"name": "Code-Review", "score": 3},
        {"name": "Maintained", "score": 10},
        {"name": "Branch-Protection", "score": -1},
        {"name": "Signed-Releases", "score": 0},
        {"name": "Fuzzing", "score": 10}
      ]
    }"#;

    #[test]
    fn la_note_est_retenue_en_dixiemes() {
        // Un entier : `6.7` en flottant vaut `6.699999809265137`, et une note
        // sur dix n'est pas une mesure continue.
        assert_eq!(lire(REPONSE).unwrap().dixiemes, 67);
        assert_eq!(lire(r#"{"score":8.26,"checks":[]}"#).unwrap().dixiemes, 83);
        assert_eq!(lire(r#"{"score":10,"checks":[]}"#).unwrap().dixiemes, 100);
        assert_eq!(lire(r#"{"score":0,"checks":[]}"#).unwrap().dixiemes, 0);
    }

    #[test]
    fn la_note_et_les_criteres_retenus_se_lisent() {
        let s = lire(REPONSE).unwrap();
        assert_eq!(s.dixiemes, 67);
        assert_eq!(s.date, "2026-09-21");
        let noms: Vec<&str> = s.criteres.iter().map(|c| c.nom.as_str()).collect();
        assert_eq!(
            noms,
            vec![
                "entretenu",
                "changements relus",
                "politique de securite",
                "versions signees"
            ]
        );
    }

    #[test]
    fn les_criteres_sans_conclusion_sont_ecartes() {
        // Scorecard rend -1 quand il n'a pas pu juger ; l'afficher comme un
        // zero accuserait le projet a tort.
        let s = lire(REPONSE).unwrap();
        assert!(!s.criteres.iter().any(|c| c.nom.contains("branche")));
    }

    #[test]
    fn un_zero_reel_est_conserve() {
        let s = lire(REPONSE).unwrap();
        let signees = s
            .criteres
            .iter()
            .find(|c| c.nom == "versions signees")
            .unwrap();
        assert_eq!(signees.note, 0);
    }

    #[test]
    fn les_criteres_qui_ne_parlent_pas_de_conduite_sont_ignores() {
        // Scorecard en publie une vingtaine, la plupart sur la chaine de
        // construction ; ils noieraient ce qu'on cherche a savoir.
        let s = lire(REPONSE).unwrap();
        assert!(!s.criteres.iter().any(|c| c.nom.contains("Fuzzing")));
    }

    #[test]
    fn une_reponse_illisible_ne_conclut_rien() {
        for j in ["", "pas du json", "{}", r#"{"checks":[]}"#] {
            assert!(
                lire(j).is_none() || lire(j).unwrap().criteres.is_empty(),
                "{j}"
            );
        }
    }

    #[tokio::test]
    async fn un_depot_mal_forme_n_appelle_pas_le_service() {
        assert!(pour("pas-un-depot").await.is_none());
        assert!(pour("a/b/c").await.is_none());
    }
}
