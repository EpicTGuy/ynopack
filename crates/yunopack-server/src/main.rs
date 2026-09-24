//! Interface web : coller une URL, suivre le pipeline.
//!
//! Le serveur n'ajoute aucune logique metier : il enrobe les memes crates que
//! le CLI. Une divergence entre les deux interfaces serait une source
//! d'incomprehension sans contrepartie.

use clap::Parser;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::response::Html;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::convert::Infallible;
use std::path::PathBuf;
use tokio_stream::wrappers::BroadcastStream;
use yunopack_server::travaux::Registre;

#[derive(Clone)]
struct Etat {
    registre: Registre,
    racine: PathBuf,
}

#[derive(Deserialize)]
struct Demande {
    url: String,
}

/// Sans cette declaration, `--help` demarrait le serveur au lieu d'afficher
/// l'aide : surprenant pour qui decouvre la commande, et genant dans un script.
#[derive(Parser)]
#[command(
    name = "yunopack-server",
    version,
    about = "Interface web du packager : coller une URL, suivre le pipeline"
)]
struct Options {
    /// Adresse d'ecoute.
    #[arg(long, env = "YNOPACK_ADDR", default_value = "127.0.0.1:8730")]
    addr: String,

    /// Repertoire ou sont ecrits les artefacts de chaque travail.
    #[arg(long, env = "YNOPACK_OUT", default_value = ".yunopack/web")]
    out: PathBuf,

    /// Prefixe sous lequel l'application est servie, par exemple `/yunopack`.
    ///
    /// Le reverse-proxy d'un serveur YunoHost transmet l'URL complete, chemin
    /// d'installation compris. Sans ce prefixe, toutes les requetes tombent en
    /// 404 des que l'application n'est pas installee a la racine du domaine.
    #[arg(long, env = "YNOPACK_BASE", default_value = "/")]
    base: String,
}

/// Ramene un prefixe a la forme attendue par `Router::nest` : commence par une
/// barre, ne finit pas par une barre, et vaut la chaine vide a la racine.
fn prefixe(brut: &str) -> String {
    let taille = brut.trim().trim_matches('/');
    if taille.is_empty() {
        String::new()
    } else {
        format!("/{taille}")
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "yunopack_server=info".into()),
        )
        .init();

    let options = Options::parse();
    let etat = Etat {
        registre: Registre::new(),
        racine: options.out.clone(),
    };

    // Les chemins sont ecrits en entier plutot que montes avec `nest` : celui-ci
    // fait repondre `/yunopack` mais pas `/yunopack/`, et c'est precisement la
    // seconde forme que transmet le reverse-proxy de YunoHost.
    let base = prefixe(&options.base);
    let mut app = Router::new()
        .route(&format!("{base}/"), get(page))
        .route(&format!("{base}/jobs"), post(creer).get(lister))
        .route(&format!("{base}/jobs/:id"), get(lire))
        .route(&format!("{base}/jobs/:id/events"), get(evenements));
    if !base.is_empty() {
        app = app.route(&base, get(page));
    }
    let app = app.with_state(etat);

    let ecoute = tokio::net::TcpListener::bind(&options.addr).await?;
    tracing::info!("yunopack sur http://{}{}/", options.addr, base);
    axum::serve(ecoute, app).await?;
    Ok(())
}

async fn page() -> Html<&'static str> {
    Html(include_str!("page.html"))
}

async fn creer(State(etat): State<Etat>, Json(d): Json<Demande>) -> Json<serde_json::Value> {
    let id = etat.registre.creer(&d.url);
    // Le pipeline tourne en tache de fond : la requete rend la main tout de
    // suite, le client suit la progression par le flux d'evenements.
    tokio::spawn(yunopack_server::pipeline::executer(
        etat.registre.clone(),
        id.clone(),
        d.url,
        etat.racine.clone(),
    ));
    Json(serde_json::json!({ "id": id }))
}

async fn lister(State(etat): State<Etat>) -> Json<serde_json::Value> {
    Json(serde_json::json!(etat.registre.liste()))
}

async fn lire(State(etat): State<Etat>, Path(id): Path<String>) -> Json<serde_json::Value> {
    match etat.registre.lire(&id) {
        Some(t) => Json(serde_json::json!(t)),
        None => Json(serde_json::json!({ "erreur": "travail inconnu" })),
    }
}

async fn evenements(
    State(etat): State<Etat>,
    Path(id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    use futures::StreamExt;

    // Le journal deja constitue est rejoue avant le direct : un client qui
    // arrive en retard, ou qui recharge la page, ne perd rien.
    let passe: Vec<_> = etat
        .registre
        .lire(&id)
        .map(|t| t.journal)
        .unwrap_or_default()
        .into_iter()
        .map(|e| Ok(Event::default().data(serde_json::to_string(&e).unwrap_or_default())))
        .collect();

    let direct = etat
        .registre
        .s_abonner(&id)
        .map(|r| {
            BroadcastStream::new(r)
                .filter_map(|e| async move {
                    e.ok().map(|e| {
                        Ok(Event::default().data(serde_json::to_string(&e).unwrap_or_default()))
                    })
                })
                .boxed()
        })
        .unwrap_or_else(|| futures::stream::empty().boxed());

    Sse::new(futures::stream::iter(passe).chain(direct))
}

#[cfg(test)]
mod tests {
    use super::prefixe;

    #[test]
    fn la_racine_ne_donne_aucun_prefixe() {
        assert_eq!(prefixe("/"), "");
        assert_eq!(prefixe(""), "");
    }

    #[test]
    fn un_chemin_est_ramene_a_la_forme_attendue_par_nest() {
        assert_eq!(prefixe("/yunopack"), "/yunopack");
        assert_eq!(prefixe("yunopack"), "/yunopack");
        assert_eq!(prefixe("/yunopack/"), "/yunopack");
        assert_eq!(prefixe(" /yunopack/ "), "/yunopack");
    }

    #[test]
    fn un_chemin_a_plusieurs_segments_reste_entier() {
        assert_eq!(prefixe("/outils/yunopack/"), "/outils/yunopack");
    }
}
