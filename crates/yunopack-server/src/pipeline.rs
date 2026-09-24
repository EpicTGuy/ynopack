//! Deroule le pipeline pour un travail, en rendant compte a chaque etape.
//!
//! La logique metier n'est pas dupliquee ici : ce module orchestre les memes
//! crates que le CLI. Une divergence entre l'interface web et la ligne de
//! commande serait une source d'incomprehension sans contrepartie.

use crate::travaux::Registre;
use std::path::PathBuf;

/// Executee dans une tache de fond ; ne rend rien, tout passe par le registre.
pub async fn executer(registre: Registre, id: String, url: String, racine: PathBuf) {
    if let Err(e) = tenter(&registre, &id, &url, &racine).await {
        // La cause profonde porte souvent l'information utile (quota de la
        // forge, depot prive) : la perdre obligerait a relancer pour rien.
        let mut message = e.to_string();
        let mut source = e.source();
        while let Some(cause) = source {
            message.push_str(&format!(" — {cause}"));
            source = cause.source();
        }
        registre.conclure(&id, false, &message);
    }
}

async fn tenter(
    registre: &Registre,
    id: &str,
    url: &str,
    racine: &std::path::Path,
) -> anyhow::Result<()> {
    let travail = racine.join(id);
    std::fs::create_dir_all(&travail)?;

    registre.avancer(id, "analyse", "recuperation du depot…");
    let recupere = ynp_forge::fetch(url).await?;
    let mut faits = ynp_analyze::analyze(recupere.forge, &recupere.tree);
    faits.selection = Some(recupere.selection.clone());
    std::fs::write(
        travail.join("facts.json"),
        serde_json::to_string_pretty(&faits)?,
    )?;

    let techno = faits.stack.primary.to_string();
    registre.avancer(
        id,
        "analyse",
        &format!(
            "{} fichiers · {techno} · source {}",
            recupere.tree.len(),
            recupere.choice.reference
        ),
    );

    registre.avancer(id, "faisabilite", "application des regles…");
    let (faisabilite, portes) =
        ynp_rules::gates::evaluate(&faits, ynp_core::DEFAULT_FEASIBILITY_THRESHOLD);
    std::fs::write(
        travail.join("report.json"),
        serde_json::to_string_pretty(&faisabilite)?,
    )?;

    if !portes.all_passed() {
        let raisons: Vec<String> = faisabilite
            .blockers()
            .map(|f| format!("{} : {}", f.id, f.title))
            .collect();
        registre.conclure(
            id,
            false,
            &format!("non faisable — {}", raisons.join(" ; ")),
        );
        return Ok(());
    }
    registre.avancer(
        id,
        "faisabilite",
        &format!(
            "{} — score {}/100",
            faisabilite.verdict.label(),
            faisabilite.score
        ),
    );

    registre.avancer(id, "specification", "arbitrages…");
    let spec = ynp_spec::build(&faits)?;
    std::fs::write(travail.join("appspec.toml"), toml::to_string_pretty(&spec)?)?;

    let manquants = spec.unresolved();
    if !manquants.is_empty() {
        let champs: Vec<&str> = manquants.iter().map(|(c, _)| c.as_str()).collect();
        registre.conclure(
            id,
            false,
            &format!(
                "{} champ(s) a completer dans appspec.toml : {}",
                manquants.len(),
                champs.join(", ")
            ),
        );
        return Ok(());
    }

    registre.avancer(id, "generation", "rendu du paquet…");
    let genere = ynp_gen::generate(&spec, &travail)?;
    registre.avancer(
        id,
        "generation",
        &format!("{} fichiers ecrits", genere.fichiers.len()),
    );

    registre.avancer(id, "verification", "conformite statique…");
    let constats = ynp_verify::verify(&genere.racine, &spec)?;
    if ynp_verify::gate(&constats).blocks_pipeline() {
        let bloquants: Vec<String> = constats
            .iter()
            .filter(|f| f.severity == ynp_core::Severity::Blocker)
            .map(|f| format!("{} : {}", f.id, f.title))
            .collect();
        registre.conclure(
            id,
            false,
            &format!("paquet non conforme — {}", bloquants.join(" ; ")),
        );
        return Ok(());
    }

    registre.conclure(
        id,
        true,
        &format!("paquet pret : {}", genere.racine.display()),
    );
    Ok(())
}
