//! Contribution au catalogue officiel de YunoHost.
//!
//! On prepare, on n'envoie pas. Une pull request vers un projet tiers engage
//! l'utilisateur, pas l'outil : ynopack produit l'entree et le corps du
//! message, et laisse l'ouverture a une decision humaine.
//!
//! La preparation est refusee tant qu'un niveau de qualite n'a pas ete mesure :
//! proposer au catalogue officiel un paquet dont on ignore le niveau fait
//! perdre du temps aux relecteurs.

use ynp_core::AppSpec;

/// Niveau minimal attendu par le catalogue officiel.
///
/// En deca, YunoHost signale l'application comme de mauvaise qualite et
/// decourage son installation.
pub const NIVEAU_MINIMAL: u8 = 4;

#[derive(Debug, thiserror::Error)]
pub enum OfficielError {
    #[error(
        "niveau {mesure} : le catalogue officiel attend au moins {NIVEAU_MINIMAL}. \
         Lancer `ynopack test` pour mesurer, ou corriger ce qui echoue."
    )]
    NiveauInsuffisant { mesure: u8 },
    #[error("niveau non mesure : lancer `ynopack test --host=<hote>` avant de proposer")]
    NiveauInconnu,
    #[error(
        "le depot doit etre heberge sur GitHub pour entrer au catalogue officiel \
         (actuellement : {url})"
    )]
    HorsGitHub { url: String },
}

/// Ce qu'il reste a faire, une fois la preparation produite.
#[derive(Debug, Clone)]
pub struct Contribution {
    /// Entree a ajouter dans `apps.toml` du depot YunoHost/apps.
    pub entree: String,
    /// Corps du message de la pull request.
    pub message: String,
}

/// Prepare la contribution, ou explique pourquoi elle n'a pas lieu d'etre.
pub fn preparer(
    spec: &AppSpec,
    url_depot: &str,
    niveau: Option<u8>,
) -> Result<Contribution, OfficielError> {
    if !url_depot.contains("github.com") {
        return Err(OfficielError::HorsGitHub {
            url: url_depot.to_string(),
        });
    }
    match niveau {
        None => return Err(OfficielError::NiveauInconnu),
        Some(n) if n < NIVEAU_MINIMAL => {
            return Err(OfficielError::NiveauInsuffisant { mesure: n })
        }
        Some(_) => {}
    }

    let id = &spec.app.id;
    let description = spec.app.description_en.value().map_or("", |v| v.as_str());

    // Le champ `level` n'est jamais renseigne a la main : un bot le met a jour
    // chaque vendredi depuis les resultats de la CI officielle.
    let entree = format!(
        "[{id}]\n\
         url = \"{url_depot}\"\n\
         category = \"\"  # a choisir dans categories.toml\n\
         # level : renseigne automatiquement par yunohost-bot depuis la CI\n"
    );

    let message = format!(
        "## Ajout de {}\n\n\
         {description}\n\n\
         - dépôt : {url_depot}\n\
         - licence amont : {}\n\
         - version incluse : {}\n\
         - niveau mesuré localement : {} (installation, service, sauvegarde, restauration, \
         désinstallation sans résidu)\n\n\
         Paquet produit par [ynopack](https://github.com/etg/yunopackage), puis relu.\n\n\
         ### Reste à faire avant fusion\n\n\
         - [ ] choisir une catégorie dans `categories.toml`\n\
         - [ ] déclencher la CI officielle par un commentaire `!testme`\n\
         - [ ] vérifier la conformité à la politique du catalogue\n",
        spec.app.name,
        spec.upstream.license.value().map_or("?", |v| v.as_str()),
        spec.app.version.value().map_or("?", |v| v.as_str()),
        niveau.unwrap_or(0),
    );

    Ok(Contribution { entree, message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ynp_core::known::Known;
    use ynp_core::spec::*;

    fn spec() -> AppSpec {
        AppSpec {
            schema_version: SPEC_SCHEMA_VERSION,
            app: AppIdentity {
                id: "demo".into(),
                name: "Demo".into(),
                description_en: Known::resolved("Une application de demonstration".into()),
                description_fr: None,
                version: Known::resolved("1.2.3".into()),
                maintainers: vec![],
            },
            upstream: Upstream {
                license: Known::resolved("MIT".into()),
                ..Default::default()
            },
            integration: Integration::default(),
            install: InstallQuestions::default(),
            resources: Resources::default(),
            runtime: Runtime::default(),
            features: Features::default(),
            docs: Docs::default(),
        }
    }

    const GITHUB: &str = "https://github.com/YunoHost-Apps/demo_ynh";

    #[test]
    fn une_contribution_complete_porte_l_entree_et_le_message() {
        let c = preparer(&spec(), GITHUB, Some(6)).unwrap();

        assert!(c.entree.starts_with("[demo]"));
        assert!(c.entree.contains(GITHUB));
        // Le niveau ne se renseigne pas a la main : un bot s'en charge.
        assert!(c.entree.contains("yunohost-bot"));
        assert!(!c.entree.contains("level ="));

        assert!(c.message.contains("Demo"));
        assert!(c.message.contains("1.2.3"));
        assert!(c.message.contains("!testme"));
    }

    #[test]
    fn un_niveau_non_mesure_empeche_la_preparation() {
        // Proposer un paquet dont on ignore le niveau fait perdre du temps
        // aux relecteurs.
        assert!(matches!(
            preparer(&spec(), GITHUB, None),
            Err(OfficielError::NiveauInconnu)
        ));
    }

    #[test]
    fn un_niveau_trop_bas_est_refuse_avec_la_marche_a_suivre() {
        let e = preparer(&spec(), GITHUB, Some(2)).unwrap_err();
        assert!(matches!(e, OfficielError::NiveauInsuffisant { mesure: 2 }));
        assert!(e.to_string().contains("ynopack test"));
    }

    #[test]
    fn un_depot_hors_github_n_entre_pas_au_catalogue_officiel() {
        let e = preparer(&spec(), "https://git.hom-e.fr/epicuser/demo_ynh", Some(8)).unwrap_err();
        assert!(matches!(e, OfficielError::HorsGitHub { .. }));
    }

    #[test]
    fn le_seuil_est_celui_du_catalogue() {
        // En deca de 4, YunoHost decourage l'installation.
        assert!(preparer(&spec(), GITHUB, Some(NIVEAU_MINIMAL)).is_ok());
        assert!(preparer(&spec(), GITHUB, Some(NIVEAU_MINIMAL - 1)).is_err());
    }
}
