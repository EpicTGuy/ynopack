//! Outil de mise au point : `cargo run -p ynp-dockerfile --example parse -- <fichier>`
//! affiche la recette extraite, en JSON.

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: parse <Dockerfile>");
        std::process::exit(2);
    };
    let content = std::fs::read_to_string(&path).expect("lecture du Dockerfile");
    let recipe = ynp_dockerfile::parse_dockerfile(&path, &content);
    println!("{}", serde_json::to_string_pretty(&recipe).unwrap());
}
