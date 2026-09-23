//! `ynopack` — packager d'applications pour YunoHost.
//!
//! Chaque sous-commande correspond a un etage du pipeline et ecrit son artefact,
//! ce qui permet de reprendre le travail en cours de route ou de corriger la
//! main a l'etage de decision.

mod report;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ynopack",
    version,
    about = "Colle une URL de depot, recupere une application YunoHost installable",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Repertoire de travail ou sont ecrits les artefacts.
    #[arg(long, short = 'o', global = true, default_value = ".ynopack")]
    out: PathBuf,

    /// Sortie JSON au lieu du rapport lisible.
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Collecte ce qui est dans le depot, sans interpretation.
    Analyze {
        /// URL du depot, sous n'importe quelle forme.
        url: String,
    },

    /// Decide si le depot est packageable, et dit pourquoi si ce n'est pas le cas.
    Assess {
        /// URL du depot. A defaut, le facts.json deja produit par `analyze` est relu.
        url: Option<String>,

        /// Score en deca duquel le verdict devient « faisable avec travail ».
        #[arg(long, default_value_t = ynp_core::DEFAULT_FEASIBILITY_THRESHOLD)]
        seuil: u8,
    },
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("\nErreur : {e}");
            // Les causes profondes portent souvent l'information utile
            // (quota d'API, depot prive), il serait dommage de les perdre.
            let mut source = e.source();
            while let Some(cause) = source {
                eprintln!("  dû à : {cause}");
                source = cause.source();
            }
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Command::Analyze { url } => analyze(url, &cli).await.map(|_| ()),
        Command::Assess { url, seuil } => assess(url.as_deref(), *seuil, &cli).await,
    }
}

async fn analyze(url: &str, cli: &Cli) -> anyhow::Result<ynp_core::facts::RepoFacts> {
    if !cli.json {
        eprintln!("Recuperation de {url} …");
    }
    let fetched = ynp_forge::fetch(url).await?;

    if !cli.json {
        eprintln!(
            "  {} fichiers, source retenue : {} ({})",
            fetched.tree.len(),
            fetched.choice.reference,
            strategie(&fetched.choice),
        );
    }

    let mut facts = ynp_analyze::analyze(fetched.forge, &fetched.tree);
    facts.selection = Some(fetched.selection.clone());

    std::fs::create_dir_all(&cli.out)?;
    let path = cli.out.join("facts.json");
    std::fs::write(&path, serde_json::to_string_pretty(&facts)?)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&facts)?);
    } else {
        print!("{}", report::facts(&facts));
        print!("{}", report::selection(&fetched.selection));
        println!("\nFaits ecrits dans {}", path.display());
    }
    Ok(facts)
}

/// Verdict de faisabilite, avec les portes G0 et G1.
///
/// Le code de sortie designe la porte en echec, pour qu'un script appelant
/// sache *ou* ca a casse sans analyser la sortie.
async fn assess(url: Option<&str>, seuil: u8, cli: &Cli) -> anyhow::Result<()> {
    let facts = match url {
        Some(u) => analyze(u, cli).await?,
        None => {
            let path = cli.out.join("facts.json");
            let text = std::fs::read_to_string(&path).map_err(|e| {
                anyhow::anyhow!(
                    "{} illisible ({e}) — lancer d'abord `ynopack analyze <url>`",
                    path.display()
                )
            })?;
            serde_json::from_str(&text)?
        }
    };

    let (feasibility, gates) = ynp_rules::gates::evaluate(&facts, seuil);

    std::fs::create_dir_all(&cli.out)?;
    std::fs::write(
        cli.out.join("report.json"),
        serde_json::to_string_pretty(&feasibility)?,
    )?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&feasibility)?);
    } else {
        print!("{}", report::feasibility(&feasibility, &gates));
    }

    let code = gates.exit_code();
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

fn strategie(c: &ynp_forge::SourceChoice) -> &'static str {
    match c.kind {
        ynp_forge::SourceKind::Release => "release",
        ynp_forge::SourceKind::Tag => "tag",
        ynp_forge::SourceKind::Commit => "commit — ni release ni tag exploitable",
    }
}
