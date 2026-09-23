//! Le contenu d'un depot, en memoire.
//!
//! Les detecteurs sont des fonctions pures de cet arbre vers des faits. Les
//! garder a l'ecart du reseau et du disque est ce qui permet de les tester un
//! par un sur des arborescences fabriquees, et de repartir le travail entre
//! plusieurs agents sans qu'ils se genent.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct RepoTree {
    files: BTreeMap<String, String>,
}

impl RepoTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }

    /// Construit un arbre a partir de paires (chemin, contenu).
    pub fn from_pairs<P, C>(pairs: impl IntoIterator<Item = (P, C)>) -> Self
    where
        P: Into<String>,
        C: Into<String>,
    {
        let mut t = Self::new();
        for (p, c) in pairs {
            t.insert(p, c);
        }
        t
    }

    pub fn paths(&self) -> Vec<String> {
        self.files.keys().cloned().collect()
    }

    pub fn text(&self, path: &str) -> Option<&str> {
        self.files.get(path).map(String::as_str)
    }

    pub fn has(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// Premier chemin existant parmi les candidats, dans l'ordre de preference.
    pub fn first_of(&self, candidates: &[&str]) -> Option<String> {
        candidates
            .iter()
            .find(|p| self.has(p))
            .map(|p| p.to_string())
    }

    /// Contenu du premier candidat present, avec son chemin.
    pub fn first_text(&self, candidates: &[&str]) -> Option<(String, &str)> {
        let p = self.first_of(candidates)?;
        let t = self.text(&p)?;
        Some((p, t))
    }

    /// Chemins dont le nom de fichier satisfait le predicat.
    pub fn find_by_name(&self, pred: impl Fn(&str) -> bool) -> Vec<String> {
        self.files
            .keys()
            .filter(|p| p.rsplit('/').next().is_some_and(&pred))
            .cloned()
            .collect()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> RepoTree {
        RepoTree::from_pairs([
            ("package.json", "{}"),
            ("src/index.js", "x"),
            ("docker/Dockerfile", "FROM x"),
        ])
    }

    #[test]
    fn l_arbre_repond_sur_la_presence_et_le_contenu() {
        let t = tree();
        assert!(t.has("package.json"));
        assert_eq!(t.text("package.json"), Some("{}"));
        assert_eq!(t.text("absent"), None);
        assert_eq!(t.len(), 3);
    }

    #[test]
    fn le_premier_candidat_present_gagne_dans_l_ordre_donne() {
        let t = tree();
        assert_eq!(
            t.first_of(&["absent", "package.json", "src/index.js"])
                .as_deref(),
            Some("package.json")
        );
        assert_eq!(t.first_of(&["absent"]), None);
        assert_eq!(t.first_text(&["package.json"]).unwrap().1, "{}");
    }

    #[test]
    fn la_recherche_par_nom_ignore_le_repertoire() {
        let t = tree();
        assert_eq!(
            t.find_by_name(|f| f.starts_with("Dockerfile")),
            vec!["docker/Dockerfile"]
        );
    }
}
