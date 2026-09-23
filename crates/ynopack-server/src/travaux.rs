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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Etat {
    EnAttente,
    EnCours,
    Termine,
    Echoue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evenement {
    pub etape: String,
    pub message: String,
    pub termine: bool,
    pub succes: bool,
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
        self.travaux.lock().unwrap().insert(id.clone(), travail);
        self.canaux.lock().unwrap().insert(id.clone(), envoi);
        id
    }

    pub fn lire(&self, id: &str) -> Option<Travail> {
        self.travaux.lock().unwrap().get(id).cloned()
    }

    pub fn liste(&self) -> Vec<Travail> {
        let mut v: Vec<Travail> = self.travaux.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| b.id.cmp(&a.id));
        v
    }

    pub fn s_abonner(&self, id: &str) -> Option<broadcast::Receiver<Evenement>> {
        self.canaux.lock().unwrap().get(id).map(|c| c.subscribe())
    }

    pub fn avancer(&self, id: &str, etape: &str, message: &str) {
        self.emettre(
            id,
            Evenement {
                etape: etape.into(),
                message: message.into(),
                termine: false,
                succes: true,
            },
        );
        if let Some(t) = self.travaux.lock().unwrap().get_mut(id) {
            t.etat = Etat::EnCours;
        }
    }

    pub fn conclure(&self, id: &str, succes: bool, message: &str) {
        self.emettre(
            id,
            Evenement {
                etape: "fin".into(),
                message: message.into(),
                termine: true,
                succes,
            },
        );
        if let Some(t) = self.travaux.lock().unwrap().get_mut(id) {
            t.etat = if succes { Etat::Termine } else { Etat::Echoue };
        }
    }

    fn emettre(&self, id: &str, evenement: Evenement) {
        if let Some(t) = self.travaux.lock().unwrap().get_mut(id) {
            t.journal.push(evenement.clone());
        }
        if let Some(canal) = self.canaux.lock().unwrap().get(id) {
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
