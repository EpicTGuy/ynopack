//! Recherche d'un logiciel par son nom, pour en retrouver le depot.
//!
//! Coller une URL suppose de l'avoir deja trouvee. Beaucoup de gens savent le
//! nom de l'outil qu'ils veulent et pas l'adresse de son depot ; les faire
//! passer par un moteur de recherche externe pour revenir coller un lien est
//! une friction sans raison.
//!
//! Les catalogues deja charges repondent en premier : ils sont locaux, sans
//! quota, et portent une description et une licence. Les forges ne sont
//! interrogees qu'ensuite, pour ce qu'ils ne connaissent pas.

use serde::Serialize;

const UA: &str = concat!("yunopack/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Serialize)]
pub struct Trouvaille {
    pub nom: String,
    pub description: String,
    pub depot: String,
    /// D'ou vient le resultat : `catalogue`, `souhaits`, `github`, `codeberg`.
    pub origine: &'static str,
    #[serde(default)]
    pub etoiles: u32,
}

/// Interroge les forges pour ce que les catalogues ne connaissent pas.
///
/// Les deux recherches sont lancees ensemble : la plus lente ne doit pas
/// retarder l'autre. Une forge qui ne repond pas ne fait pas echouer la
/// recherche — elle rend simplement moins de resultats.
pub async fn sur_les_forges(terme: &str, exclure: &[String]) -> Vec<Trouvaille> {
    let Ok(client) = reqwest::Client::builder()
        .user_agent(UA)
        .timeout(std::time::Duration::from_secs(12))
        .build()
    else {
        return Vec::new();
    };

    let (gh, cb) = tokio::join!(github(&client, terme), codeberg(&client, terme),);

    let mut out: Vec<Trouvaille> = gh.into_iter().chain(cb).collect();
    out.retain(|t| !exclure.iter().any(|d| d.eq_ignore_ascii_case(&t.depot)));
    out.sort_by_key(|t| std::cmp::Reverse(t.etoiles));
    out.truncate(25);
    out
}

async fn github(client: &reqwest::Client, terme: &str) -> Vec<Trouvaille> {
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars&per_page=15",
        urlencoding(terme)
    );
    let mut req = client.get(url);
    if let Ok(t) = std::env::var("GITHUB_TOKEN") {
        if !t.is_empty() {
            req = req.bearer_auth(t);
        }
    }
    let Ok(r) = req.send().await else {
        return Vec::new();
    };
    if !r.status().is_success() {
        return Vec::new();
    }
    let Ok(v) = r.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    v["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some(Trouvaille {
                        nom: i["name"].as_str()?.to_string(),
                        description: i["description"].as_str().unwrap_or_default().to_string(),
                        depot: i["html_url"].as_str()?.to_string(),
                        origine: "github",
                        etoiles: i["stargazers_count"].as_u64().unwrap_or(0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

async fn codeberg(client: &reqwest::Client, terme: &str) -> Vec<Trouvaille> {
    let url = format!(
        "https://codeberg.org/api/v1/repos/search?q={}&limit=10&sort=stars&order=desc",
        urlencoding(terme)
    );
    let Ok(r) = client.get(url).send().await else {
        return Vec::new();
    };
    if !r.status().is_success() {
        return Vec::new();
    }
    let Ok(v) = r.json::<serde_json::Value>().await else {
        return Vec::new();
    };
    v["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some(Trouvaille {
                        nom: i["name"].as_str()?.to_string(),
                        description: i["description"].as_str().unwrap_or_default().to_string(),
                        depot: i["html_url"].as_str()?.to_string(),
                        origine: "codeberg",
                        etoiles: i["stars_count"].as_u64().unwrap_or(0) as u32,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Encodage minimal d'un terme de recherche.
///
/// Seuls les caracteres qui casseraient l'URL sont traites ; le reste passe
/// tel quel, ce qui garde les requetes lisibles dans les journaux.
fn urlencoding(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "+".to_string(),
            other => other
                .to_string()
                .bytes()
                .map(|b| format!("%{b:02X}"))
                .collect(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_terme_se_encode_sans_casser_l_url() {
        assert_eq!(urlencoding("nextcloud"), "nextcloud");
        assert_eq!(urlencoding("gestion de notes"), "gestion+de+notes");
        assert_eq!(urlencoding("c++"), "c%2B%2B");
        assert_eq!(urlencoding("café"), "caf%C3%A9");
        assert_eq!(urlencoding("a/b?c=d&e"), "a%2Fb%3Fc%3Dd%26e");
    }

    #[test]
    fn les_depots_deja_connus_sont_ecartes_sans_tenir_compte_de_la_casse() {
        let mut v = vec![
            Trouvaille {
                nom: "A".into(),
                description: String::new(),
                depot: "https://github.com/Owner/Repo".into(),
                origine: "github",
                etoiles: 10,
            },
            Trouvaille {
                nom: "B".into(),
                description: String::new(),
                depot: "https://github.com/autre/repo".into(),
                origine: "github",
                etoiles: 5,
            },
        ];
        let exclure = ["https://github.com/owner/repo".to_string()];
        v.retain(|t| !exclure.iter().any(|d| d.eq_ignore_ascii_case(&t.depot)));
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].nom, "B");
    }
}
