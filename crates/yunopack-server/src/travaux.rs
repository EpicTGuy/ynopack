//! File de travaux.
//!
//! Le pipeline est sequentiel et long : analyser un depot prend une dizaine de
//! secondes, l'installer plusieurs minutes. Les lancer en parallele saturerait
//! la machine et le quota de la forge, d'ou une file a un seul executant.
//!
//! L'etat vit en memoire : un travail est une session de travail, pas une
//! donnee a conserver. Ce qui doit survivre — faits, specification, paquet —
//! est ecrit sur disque par le pipeline lui-meme.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use ynp_core::spec::Arbitrage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Etat {
    EnAttente,
    EnCours,
    /// Le pipeline a fait tout ce qu'il pouvait seul : il reste des arbitrages
    /// que personne d'autre que l'utilisateur ne peut rendre.
    EnAttenteDeDecision,
    Termine,
    Echoue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evenement {
    pub etape: String,
    pub message: String,
    pub termine: bool,
    pub succes: bool,
    /// Les champs a renseigner, quand l'etape est une demande de decision.
    /// Portes par l'evenement plutot que rappeles a part : le journal rejoue a
    /// la reconnexion reconstruit ainsi le formulaire tel quel.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arbitrages: Vec<Arbitrage>,
    /// Chemin du paquet produit, quand l'etape conclut un succes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paquet: Option<String>,
}

impl Evenement {
    fn etape(etape: &str, message: &str) -> Self {
        Self {
            etape: etape.into(),
            message: message.into(),
            termine: false,
            succes: true,
            arbitrages: Vec::new(),
            paquet: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Travail {
    pub id: String,
    pub url: String,
    pub etat: Etat,
    pub journal: Vec<Evenement>,
}

/// Registre partage entre le serveur et l'executant.
#[derive(Clone, Default)]
pub struct Registre {
    travaux: Arc<Mutex<HashMap<String, Travail>>>,
    canaux: Arc<Mutex<HashMap<String, broadcast::Sender<Evenement>>>>,
}

/// Prend un verrou en se remettant d'un eventuel empoisonnement.
///
/// Un `unwrap()` sur `lock()` suffirait, mais il transforme la panique d'une
/// seule tache en panne definitive du serveur : une fois le mutex empoisonne,
/// tout appel ulterieur panique a son tour. Or les donnees protegees ici sont
/// une table de travaux ; elles restent structurellement valides meme si une
/// tache s'est interrompue au milieu. Recuperer est donc correct, et bien
/// preferable a l'arret du service.
fn verrou<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Registre {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn creer(&self, url: &str) -> String {
        let id = identifiant();
        let travail = Travail {
            id: id.clone(),
            url: url.to_string(),
            etat: Etat::EnAttente,
            journal: Vec::new(),
        };
        // La capacite du canal absorbe le journal complet d'un pipeline : un
        // abonne lent ne doit pas perdre d'etapes.
        let (envoi, _) = broadcast::channel(64);
        verrou(&self.travaux).insert(id.clone(), travail);
        verrou(&self.canaux).insert(id.clone(), envoi);
        id
    }

    pub fn lire(&self, id: &str) -> Option<Travail> {
        verrou(&self.travaux).get(id).cloned()
    }

    pub fn liste(&self) -> Vec<Travail> {
        let mut v: Vec<Travail> = verrou(&self.travaux).values().cloned().collect();
        v.sort_by(|a, b| b.id.cmp(&a.id));
        v
    }

    pub fn s_abonner(&self, id: &str) -> Option<broadcast::Receiver<Evenement>> {
        verrou(&self.canaux).get(id).map(|c| c.subscribe())
    }

    pub fn avancer(&self, id: &str, etape: &str, message: &str) {
        self.emettre(id, Evenement::etape(etape, message));
        if let Some(t) = verrou(&self.travaux).get_mut(id) {
            t.etat = Etat::EnCours;
        }
    }

    /// Suspend le travail sur des arbitrages a rendre.
    ///
    /// Ce n'est pas un echec : rien n'a echoue, l'outil a simplement atteint la
    /// limite de ce qu'il peut etablir seul. Le distinguer d'un echec change ce
    /// que l'interface propose — un formulaire plutot qu'un message d'erreur.
    pub fn demander(&self, id: &str, arbitrages: Vec<Arbitrage>) {
        let message = format!(
            "{} champ(s) que le depot ne permet pas de determiner",
            arbitrages.len()
        );
        self.emettre(
            id,
            Evenement {
                arbitrages,
                ..Evenement::etape("decision", &message)
            },
        );
        if let Some(t) = verrou(&self.travaux).get_mut(id) {
            t.etat = Etat::EnAttenteDeDecision;
        }
    }

    pub fn conclure(&self, id: &str, succes: bool, message: &str) {
        self.conclure_avec(id, succes, message, None);
    }

    pub fn conclure_avec(&self, id: &str, succes: bool, message: &str, paquet: Option<String>) {
        self.emettre(
            id,
            Evenement {
                termine: true,
                succes,
                paquet,
                ..Evenement::etape("fin", message)
            },
        );
        if let Some(t) = verrou(&self.travaux).get_mut(id) {
            t.etat = if succes { Etat::Termine } else { Etat::Echoue };
        }
    }

    /// Vrai si le travail attend une decision : seul etat ou une reponse a un
    /// sens. Repondre a un travail termine ou en cours serait une confusion.
    pub fn attend_une_decision(&self, id: &str) -> bool {
        verrou(&self.travaux)
            .get(id)
            .is_some_and(|t| t.etat == Etat::EnAttenteDeDecision)
    }

    fn emettre(&self, id: &str, evenement: Evenement) {
        if let Some(t) = verrou(&self.travaux).get_mut(id) {
            t.journal.push(evenement.clone());
        }
        if let Some(canal) = verrou(&self.canaux).get(id) {
            // Aucun abonne : l'evenement reste dans le journal, qu'un client
            // arrivant en retard peut relire.
            let _ = canal.send(evenement);
        }
    }
}

/// Identifiant court, croissant dans le temps pour que le tri soit naturel.
fn identifiant() -> String {
    let micros = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0);
    format!("{micros:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_travail_cree_est_en_attente_et_retrouvable() {
        let r = Registre::new();
        let id = r.creer("https://github.com/x/y");
        let t = r.lire(&id).unwrap();

        assert_eq!(t.etat, Etat::EnAttente);
        assert_eq!(t.url, "https://github.com/x/y");
        assert!(t.journal.is_empty());
    }

    #[test]
    fn le_journal_conserve_les_etapes_dans_l_ordre() {
        let r = Registre::new();
        let id = r.creer("u");
        r.avancer(&id, "analyse", "655 fichiers");
        r.avancer(&id, "faisabilite", "score 100");
        r.conclure(&id, true, "paquet pret");

        let t = r.lire(&id).unwrap();
        assert_eq!(t.etat, Etat::Termine);
        let etapes: Vec<&str> = t.journal.iter().map(|e| e.etape.as_str()).collect();
        assert_eq!(etapes, vec!["analyse", "faisabilite", "fin"]);
    }

    #[test]
    fn un_echec_marque_le_travail_sans_effacer_le_journal() {
        let r = Registre::new();
        let id = r.creer("u");
        r.avancer(&id, "analyse", "ok");
        r.conclure(&id, false, "non faisable : DB002");

        let t = r.lire(&id).unwrap();
        assert_eq!(t.etat, Etat::Echoue);
        assert_eq!(t.journal.len(), 2);
        assert!(!t.journal.last().unwrap().succes);
    }

    #[tokio::test]
    async fn un_abonne_recoit_les_etapes_en_direct() {
        let r = Registre::new();
        let id = r.creer("u");
        let mut abonne = r.s_abonner(&id).unwrap();

        r.avancer(&id, "analyse", "en cours");
        let e = abonne.recv().await.unwrap();
        assert_eq!(e.etape, "analyse");
        assert!(!e.termine);
    }

    #[test]
    fn un_travail_inconnu_ne_fait_pas_paniquer() {
        let r = Registre::new();
        assert!(r.lire("inexistant").is_none());
        assert!(r.s_abonner("inexistant").is_none());
        // Avancer sur un inconnu est sans effet, pas une panique.
        r.avancer("inexistant", "x", "y");
    }

    #[test]
    fn les_travaux_sont_listes_du_plus_recent_au_plus_ancien() {
        let r = Registre::new();
        let a = r.creer("premier");
        std::thread::sleep(std::time::Duration::from_micros(2));
        let b = r.creer("second");

        let liste = r.liste();
        assert_eq!(liste[0].id, b);
        assert_eq!(liste[1].id, a);
    }
}

#[cfg(test)]
mod robustesse {
    use super::*;

    #[test]
    fn un_verrou_empoisonne_n_arrete_pas_le_serveur() {
        // Si une tache panique en tenant le verrou, `lock().unwrap()` ferait
        // paniquer toutes les requetes suivantes. Le registre doit survivre.
        let registre = Registre::new();
        let id = registre.creer("https://github.com/x/y");

        let empoisonneur = registre.clone();
        let _ = std::thread::spawn(move || {
            let _garde = verrou(&empoisonneur.travaux);
            panic!("tache interrompue au milieu");
        })
        .join();

        // Le registre reste utilisable, et le travail est intact.
        let t = registre.lire(&id).expect("le travail doit survivre");
        assert_eq!(t.url, "https://github.com/x/y");
        registre.avancer(&id, "analyse", "toujours vivant");
        assert_eq!(registre.lire(&id).unwrap().journal.len(), 1);
    }
}
