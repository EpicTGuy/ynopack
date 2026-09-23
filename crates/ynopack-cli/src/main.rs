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
        Command::Analyze { url } => analyze(url, &cli).await,
    }
}

async fn analyze(url: &str, cli: &Cli) -> anyhow::Result<()> {
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

    let facts = ynp_analyze::analyze(fetched.forge, &fetched.tree);

    std::fs::create_dir_all(&cli.out)?;
    let path = cli.out.join("facts.json");
    std::fs::write(&path, serde_json::to_string_pretty(&facts)?)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&facts)?);
    } else {
        print!("{}", report::facts(&facts));
        println!("\n  source          {}", fetched.choice.url);
        println!("  sha256          {}", fetched.sha256);
        println!("\nFaits ecrits dans {}", path.display());
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
