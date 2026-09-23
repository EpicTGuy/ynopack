//! Outil de mise au point : `cargo run -p ynp-dockerfile --example compose_probe -- <fichiers>`
//! resume ce que chaque compose revele.

fn main() {
    for path in std::env::args().skip(1) {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let name = path.rsplit('/').next().unwrap_or(&path);
        let hint = name.split('.').next();

        match ynp_dockerfile::parse_compose(&path, &content, hint) {
            None => println!("  {name:24} YAML illisible"),
            Some(facts) => {
                let svc = ynp_dockerfile::services_from_compose(&facts);
                let apps: Vec<&str> = facts
                    .services
                    .iter()
                    .filter(|s| s.is_app)
                    .map(|s| s.name.as_str())
                    .collect();
                let ports: Vec<u16> = facts
                    .services
                    .iter()
                    .filter(|s| s.is_app)
                    .flat_map(|s| s.ports.clone())
                    .collect();
                println!(
                    "  {name:24} app={apps:?} db={:?} redis={} ports={ports:?} non_supportes={:?}",
                    svc.database, svc.needs_redis, svc.unsupported
                );
            }
        }
    }
}
