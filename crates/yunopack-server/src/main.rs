//! Interface web : coller une URL, suivre le pipeline.
//!
//! Le serveur n'ajoute aucune logique metier : il enrobe les memes crates que
//! le CLI. Une divergence entre les deux interfaces serait une source
//! d'incomprehension sans contrepartie.

use clap::Parser;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::sse::{Event, Sse};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use yunopack_server::travaux::Registre;

#[derive(Clone)]
struct Etat {
    registre: Registre,
    racine: PathBuf,
    evaluateur: yunopack_server::evaluation::Evaluateur,
    catalogue: Arc<std::sync::Mutex<Option<Arc<ynp_forge::alternatives::Catalogue>>>>,
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

    /// Ne pas evaluer la liste de souhaits en tache de fond.
    ///
    /// Le defrichage consomme le quota de la forge ; on veut pouvoir s'en
    /// passer sur une machine qui n'a que l'API publique et d'autres usages.
    #[arg(long, env = "YNOPACK_SANS_DEFRICHAGE")]
    sans_defrichage: bool,
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
    // Le cache vit a cote des travaux : un seul repertoire a sauvegarder, et
    // le paquet YunoHost le place deja dans son repertoire de donnees.
    let cache = yunopack_server::cache::Cache::new(&options.out.join("cache"));
    let etat = Etat {
        registre: Registre::new(),
        racine: options.out.clone(),
        evaluateur: yunopack_server::evaluation::Evaluateur::new(cache),
        catalogue: Arc::new(std::sync::Mutex::new(None)),
    };

    // Les chemins sont ecrits en entier plutot que montes avec `nest` : celui-ci
    // fait repondre `/yunopack` mais pas `/yunopack/`, et c'est precisement la
    // seconde forme que transmet le reverse-proxy de YunoHost.
    let base = prefixe(&options.base);
    let mut app = Router::new()
        .route(&format!("{base}/"), get(page))
        .route(&format!("{base}/jobs"), post(creer).get(lister))
        .route(&format!("{base}/jobs/:id"), get(lire))
        .route(&format!("{base}/jobs/:id/events"), get(evenements))
        .route(&format!("{base}/jobs/:id/reponses"), post(repondre))
        .route(&format!("{base}/jobs/:id/paquet.tar.gz"), get(paquet))
        .route(&format!("{base}/jobs/:id/publier"), post(publier))
        .route(&format!("{base}/forge"), get(forge))
        .route(&format!("{base}/wishlist"), get(souhaits))
        .route(&format!("{base}/alternatives"), get(alternatives))
        .route(&format!("{base}/suggestions"), get(suggestions))
        .route(&format!("{base}/recherche"), get(recherche))
        .route(
            &format!("{base}/evaluations"),
            get(evaluations).post(evaluer),
        )
        .route(&format!("{base}/icone"), get(icone));
    if !base.is_empty() {
        app = app.route(&base, get(page));
    }
    let app = app.with_state(etat.clone());

    // Defricher la liste de souhaits des le demarrage : sans cela, une
    // installation neuve n'affiche aucun score tant que personne n'a fait
    // defiler les listes. Le rythme est celui que le quota de la forge
    // permet, et les demandes directes passent devant.
    if !options.sans_defrichage {
        let evaluateur = etat.evaluateur.clone();
        tokio::spawn(async move {
            match ynp_forge::wishlist::recuperer().await {
                Ok(liste) => {
                    let urls: Vec<String> = liste
                        .iter()
                        .filter(|s| s.analysable())
                        .map(|s| s.upstream.clone())
                        .collect();
                    let n = evaluateur.defricher(urls);
                    tracing::info!("defrichage : {n} depot(s) de la liste de souhaits en attente");
                }
                Err(e) => tracing::warn!("liste de souhaits indisponible : {e}"),
            }
        });
    }

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

/// Ce que la communaute YunoHost attend, moins ce qui existe deja.
///
/// L'interet de l'exposer ici est de supprimer une etape : plutot que de
/// chercher quoi packager puis de recopier une URL, on clique sur une demande.
async fn souhaits() -> (StatusCode, Json<serde_json::Value>) {
    match ynp_forge::wishlist::recuperer().await {
        Ok(liste) => {
            let items: Vec<_> = liste
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "description": s.description,
                        "repo": s.upstream,
                        "analysable": s.analysable(),
                        "en_cours": s.en_cours(),
                        "votes": s.votes,
                        "deja_package": s.deja_package,
                        "id_yunohost": s.id_yunohost,
                    })
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(items)))
        }
        Err(e) => refus(StatusCode::BAD_GATEWAY, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct Recherche {
    q: String,
}

/// Le catalogue externe, charge une fois puis reutilise.
///
/// Son archive fait plusieurs mega-octets et ne bouge qu'au rythme des
/// contributions : la retelecharger a chaque recherche ferait attendre
/// l'utilisateur pour rien.
async fn catalogue_externe(etat: &Etat) -> Result<Arc<ynp_forge::alternatives::Catalogue>, String> {
    if let Some(c) = verrou_catalogue(&etat.catalogue).clone() {
        return Ok(c);
    }
    let c = Arc::new(
        ynp_forge::alternatives::Catalogue::charger()
            .await
            .map_err(|e| e.to_string())?,
    );
    *verrou_catalogue(&etat.catalogue) = Some(c.clone());
    Ok(c)
}

fn verrou_catalogue<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Les logiciels auto-hebergeables proches d'un autre.
///
/// Ceux deja au catalogue YunoHost ne sont plus ecartes mais signales : savoir
/// qu'une alternative existe deja est au moins aussi utile que d'apprendre
/// qu'il reste a la packager, et c'est ce qui permet de partir d'une
/// application qu'on utilise pour en decouvrir de proches.
async fn alternatives(
    State(etat): State<Etat>,
    axum::extract::Query(r): axum::extract::Query<Recherche>,
) -> (StatusCode, Json<serde_json::Value>) {
    let catalogue = match catalogue_externe(&etat).await {
        Ok(c) => c,
        Err(e) => return refus(StatusCode::BAD_GATEWAY, &e),
    };

    // Les alternatives d'abord ; a defaut, une recherche par nom, pour que
    // saisir un terme approximatif rende quelque chose plutot que rien.
    let proches = catalogue.alternatives(&r.q);
    let trouves = if proches.is_empty() {
        catalogue.chercher(&r.q)
    } else {
        proches
    };

    let items: Vec<_> = trouves
        .iter()
        .take(60)
        .map(|l| {
            serde_json::json!({
                "name": l.name,
                "description": l.description,
                "repo": l.source_code_url,
                "site": l.website_url,
                "stars": l.stargazers_count,
                "license": l.licenses.first(),
                "libre": l.licence_libre(),
                "analysable": l.analysable(),
                "deja_package": l.deja_package,
                "id_yunohost": l.id_yunohost,
                "abandonne": l.abandonne(),
                "jours_sans_activite": l.jours_sans_activite(),
                "derniere_activite": l.updated_at,
            })
        })
        .collect();
    (StatusCode::OK, Json(serde_json::json!(items)))
}

/// Cherche un logiciel par son nom, et rend les depots qui correspondent.
///
/// Coller une URL suppose de l'avoir deja trouvee. Beaucoup de gens
/// connaissent le nom de l'outil qu'ils veulent, pas l'adresse de son depot.
/// Les catalogues locaux repondent en premier — sans quota, avec description
/// et licence — puis les forges, pour ce qu'ils ignorent.
async fn recherche(
    State(etat): State<Etat>,
    axum::extract::Query(r): axum::extract::Query<Recherche>,
) -> Json<serde_json::Value> {
    let terme = r.q.trim();
    if terme.len() < 2 {
        return Json(serde_json::json!([]));
    }

    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut vus: Vec<String> = Vec::new();

    if let Ok(c) = catalogue_externe(&etat).await {
        for l in c.chercher(terme).into_iter().take(15) {
            if l.source_code_url.is_empty() {
                continue;
            }
            vus.push(l.source_code_url.clone());
            out.push(serde_json::json!({
                "nom": l.name,
                "description": l.description,
                "depot": l.source_code_url,
                "origine": "catalogue",
                "etoiles": l.stargazers_count,
                "deja_package": l.deja_package,
                "id_yunohost": l.id_yunohost,
            }));
        }
    }

    for t in ynp_forge::recherche::sur_les_forges(terme, &vus).await {
        out.push(serde_json::json!({
            "nom": t.nom,
            "description": t.description,
            "depot": t.depot,
            "origine": t.origine,
            "etoiles": t.etoiles,
            "deja_package": false,
        }));
    }

    Json(serde_json::json!(out))
}

/// Les noms du catalogue externe, pour l'autocompletion.
async fn suggestions(State(etat): State<Etat>) -> (StatusCode, Json<serde_json::Value>) {
    match catalogue_externe(&etat).await {
        Ok(c) => (StatusCode::OK, Json(serde_json::json!(c.noms()))),
        Err(e) => refus(StatusCode::BAD_GATEWAY, &e),
    }
}

#[derive(Deserialize)]
struct Depots {
    /// Liste d'URL separees par des virgules.
    depots: String,
}

/// Ce que le cache sait deja de ces depots. N'evalue rien.
///
/// Separer la consultation de la demande est ce qui permet a la liste de
/// s'afficher instantanement : on montre ce qu'on sait, et on demande le reste
/// seulement pour les lignes que l'utilisateur regarde.
async fn evaluations(
    State(etat): State<Etat>,
    axum::extract::Query(d): axum::extract::Query<Depots>,
) -> Json<serde_json::Value> {
    let depots: Vec<String> = d
        .depots
        .split(',')
        .filter_map(yunopack_server::cache::depot_de)
        .collect();
    Json(serde_json::json!({
        "fiches": etat.evaluateur.cache().lot(&depots),
        "en_attente": etat.evaluateur.en_attente(),
        "a_defricher": etat.evaluateur.reste_a_defricher(),
        "connues": etat.evaluateur.cache().nombre(),
    }))
}

#[derive(Deserialize)]
struct DemandeEvaluation {
    url: String,
    /// Reevaluer meme si le depot est deja en cache.
    #[serde(default)]
    refaire: bool,
}

/// Met un depot en file d'evaluation. Rend la main aussitot.
async fn evaluer(
    State(etat): State<Etat>,
    Json(d): Json<DemandeEvaluation>,
) -> Json<serde_json::Value> {
    let etat_demande = etat.evaluateur.demander(&d.url, d.refaire);
    Json(serde_json::json!({
        "etat": etat_demande,
        "en_attente": etat.evaluateur.en_attente(),
    }))
}

#[derive(Deserialize)]
struct DemandeIcone {
    depot: String,
}

/// Relaie l'icone d'un projet, en la mettant en cache.
async fn icone(
    State(etat): State<Etat>,
    axum::extract::Query(d): axum::extract::Query<DemandeIcone>,
) -> Response {
    match yunopack_server::evaluation::icone(etat.evaluateur.cache(), &d.depot).await {
        Some(octets) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                // L'avatar d'un compte ne change presque jamais ; le
                // reconsulter a chaque affichage de la liste serait absurde.
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            octets,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Applique les reponses de l'utilisateur, puis relance le pipeline.
///
/// Les reponses sont ecrites dans le meme `appspec.toml` que celui qu'edite
/// `yunopack repondre` : le formulaire web n'est pas un chemin parallele, c'est
/// la meme decision prise ailleurs.
async fn repondre(
    State(etat): State<Etat>,
    Path(id): Path<String>,
    Json(reponses): Json<BTreeMap<String, String>>,
) -> (StatusCode, Json<serde_json::Value>) {
    if !etat.registre.attend_une_decision(&id) {
        return refus(
            StatusCode::CONFLICT,
            "ce travail n'attend pas de decision — le relancer depuis son URL",
        );
    }

    let travail = etat.racine.join(&id);
    let chemin = yunopack_server::pipeline::chemin_appspec(&travail);
    let Ok(texte) = std::fs::read_to_string(&chemin) else {
        return refus(
            StatusCode::NOT_FOUND,
            "specification introuvable — relancer l'analyse",
        );
    };
    let mut spec: ynp_core::AppSpec = match toml::from_str(&texte) {
        Ok(s) => s,
        Err(e) => return refus(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Tout ou rien : appliquer la moitie des reponses laisserait une
    // specification a moitie decidee, sans que personne sache laquelle.
    for (champ, valeur) in &reponses {
        if let Err(e) = spec.repondre(champ, valeur) {
            return refus(StatusCode::BAD_REQUEST, &e.to_string());
        }
    }

    let rendu = match toml::to_string_pretty(&spec) {
        Ok(t) => t,
        Err(e) => return refus(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    if let Err(e) = std::fs::write(&chemin, rendu) {
        return refus(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    etat.registre.avancer(
        &id,
        "decision",
        &format!("{} reponse(s) prises en compte", reponses.len()),
    );
    tokio::spawn(yunopack_server::pipeline::reprendre(
        etat.registre.clone(),
        id,
        travail,
    ));
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "repris": true })),
    )
}

fn refus(code: StatusCode, message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (code, Json(serde_json::json!({ "erreur": message })))
}

/// Ou l'interface peut publier, si elle le peut.
///
/// Le service tourne sous un utilisateur systeme sans trousseau SSH : sans
/// jeton de forge dans son environnement, il n'a aucun moyen de pousser. Le
/// dire d'emblee vaut mieux qu'un bouton qui echoue.
async fn forge() -> Json<serde_json::Value> {
    match ynp_publish::forge::Forge::depuis_environnement("forgejo") {
        Some(f) if f.jeton.is_some() => Json(serde_json::json!({
            "configuree": true,
            "url": f.url,
            "proprietaire": f.proprietaire,
        })),
        Some(_) => Json(serde_json::json!({
            "configuree": false,
            "manque": "FORGEJO_TOKEN",
        })),
        None => Json(serde_json::json!({
            "configuree": false,
            "manque": "FORGEJO_URL et FORGEJO_OWNER",
        })),
    }
}

/// Pousse le paquet produit sur la forge configuree.
///
/// Publier est la seule facon d'atteindre une CI : c'est une machine jetable
/// qui doit installer un paquet non relu, jamais celle qui heberge
/// l'interface.
async fn publier(
    State(etat): State<Etat>,
    Path(id): Path<String>,
) -> (StatusCode, Json<serde_json::Value>) {
    let Some(f) = ynp_publish::forge::Forge::depuis_environnement("forgejo") else {
        return refus(
            StatusCode::PRECONDITION_FAILED,
            "aucune forge configuree — definir FORGEJO_URL, FORGEJO_OWNER et FORGEJO_TOKEN",
        );
    };
    let Some(paquet) = paquet_du_travail(&etat.racine.join(&id)) else {
        return refus(StatusCode::NOT_FOUND, "aucun paquet pour ce travail");
    };
    let nom = paquet
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "paquet_ynh".to_string());

    if let Err(e) = f
        .creer_depot(&nom, "Paquet YunoHost genere par yunopack")
        .await
    {
        return refus(StatusCode::BAD_GATEWAY, &e.to_string());
    }
    let Some(distant) = f.url_push_https(&nom) else {
        return refus(
            StatusCode::PRECONDITION_FAILED,
            "FORGEJO_TOKEN absent : impossible de pousser sans cle SSH ni jeton",
        );
    };

    let paquet_clone = paquet.clone();
    let nom_clone = nom.clone();
    let pousse = tokio::task::spawn_blocking(move || {
        ynp_publish::forge::pousser(
            &paquet_clone,
            &distant,
            "main",
            &format!("{nom_clone} genere par yunopack"),
        )
    })
    .await;

    match pousse {
        Ok(Ok(_)) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "url": f.url_https(&nom),
                "depot": nom,
            })),
        ),
        Ok(Err(e)) => refus(StatusCode::BAD_GATEWAY, &e.to_string()),
        Err(e) => refus(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Le paquet produit, en archive.
///
/// Sans cela, le seul moyen de recuperer le travail serait d'avoir un acces
/// shell a la machine qui heberge le serveur — ce qui vide l'interface web de
/// son interet pour qui ne l'a pas.
async fn paquet(State(etat): State<Etat>, Path(id): Path<String>) -> Response {
    let travail = etat.racine.join(&id);
    let Some(dossier) = paquet_du_travail(&travail) else {
        return (StatusCode::NOT_FOUND, "aucun paquet pour ce travail").into_response();
    };
    let nom = dossier
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "paquet".to_string());

    let archive = match archiver(&dossier, &nom) {
        Ok(a) => a,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };

    (
        [
            (header::CONTENT_TYPE, "application/gzip".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{nom}.tar.gz\""),
            ),
        ],
        archive,
    )
        .into_response()
}

/// Le repertoire `<app>_ynh` produit dans le repertoire de travail.
fn paquet_du_travail(travail: &std::path::Path) -> Option<PathBuf> {
    std::fs::read_dir(travail)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().ends_with("_ynh"))
        })
}

fn archiver(dossier: &std::path::Path, nom: &str) -> std::io::Result<Vec<u8>> {
    let encodeur = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut tar = tar::Builder::new(encodeur);
    // Les scripts doivent rester executables a l'arrivee : `append_dir_all`
    // conserve les permissions, une reconstruction fichier par fichier non.
    tar.append_dir_all(nom, dossier)?;
    tar.into_inner()?.finish()
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
