//! Le parseur confronte aux Dockerfiles qui l'ont mis en defaut.
//!
//! Chaque cas ci-dessous a d'abord ete un bug constate sur l'application
//! nommee. Les conserver garantit qu'une correction ne reintroduit pas le
//! defaut d'a cote.

use ynp_core::tree::RepoTree;
use ynp_dockerfile::{choose_dockerfile, parse_dockerfile};

fn charger(nom: &str) -> String {
    let chemin = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/dockerfiles/"
    );
    std::fs::read_to_string(format!("{chemin}{nom}")).unwrap_or_else(|e| panic!("{nom} : {e}"))
}

#[test]
fn miniflux_base_alpine_expose_son_port_et_son_binaire() {
    let r = parse_dockerfile("Dockerfile", &charger("miniflux.Dockerfile"));

    assert_eq!(r.expose, vec![8080]);
    assert_eq!(r.start_command().as_deref(), Some("/usr/bin/miniflux"));
    // Les paquets Alpine sont collectes a part, pour etre traduits ensuite.
    assert!(r.apk_packages.contains(&"build-base".to_string()));
    assert!(r.apt_packages.is_empty());
}

#[test]
fn paperless_recupere_ses_paquets_ranges_dans_des_arg() {
    // Sans substitution, la detection tombait a zero dependance.
    let r = parse_dockerfile("Dockerfile", &charger("paperless.Dockerfile"));

    assert!(
        r.apt_packages.len() > 20,
        "{} paquets seulement",
        r.apt_packages.len()
    );
    assert!(r.apt_packages.contains(&"tesseract-ocr".to_string()));
    // L'etape de runtime part d'un alias local : c'est l'image reelle qui doit
    // ressortir, pas « s6-overlay-base ».
    assert_ne!(r.runtime_stage().unwrap().image, "s6-overlay-base");
}

#[test]
fn vikunja_sur_scratch_reste_lisible() {
    let r = parse_dockerfile("Dockerfile", &charger("vikunja.Dockerfile"));

    assert_eq!(r.runtime_stage().unwrap().image, "scratch");
    assert_eq!(r.expose, vec![3456]);
    assert!(r.start_command().is_some());
}

#[test]
fn linkwarden_ignore_les_options_buildkit() {
    let r = parse_dockerfile("Dockerfile", &charger("linkwarden.Dockerfile"));

    assert!(
        r.build_steps.iter().all(|s| !s.starts_with("--mount")),
        "options BuildKit retenues : {:?}",
        r.build_steps
    );
    assert_eq!(r.expose, vec![3000]);
}

#[test]
fn grist_ne_laisse_pas_de_fragments_de_blocs_shell() {
    let r = parse_dockerfile("Dockerfile", &charger("grist.Dockerfile"));

    let fragments = ["esac", "case", "fi", "then", "{", "}"];
    for etape in &r.build_steps {
        let premier = etape.split_whitespace().next().unwrap_or("");
        assert!(
            !fragments.contains(&premier),
            "fragment de syntaxe retenu comme etape : {etape}"
        );
    }
    assert_eq!(r.expose, vec![8484]);
}

#[test]
fn entre_plusieurs_dockerfiles_celui_du_produit_est_choisi() {
    // Cas reel de miniflux, qui en compte quatre. Celui du conditionnement
    // Debian, plus proche de la racine, l'emportait et faisait passer
    // devscripts et dh-make pour des dependances de l'application.
    let conditionnement = "FROM golang:1\n                           RUN apt-get install -y devscripts dh-make debhelper\n                           CMD [\"/src/packaging/debian/build.sh\"]\n";

    let tree = RepoTree::from_pairs([
        ("packaging/debian/Dockerfile", conditionnement.to_string()),
        (
            "packaging/docker/alpine/Dockerfile",
            charger("miniflux.Dockerfile"),
        ),
    ]);

    let choisi = choose_dockerfile(&tree).unwrap();
    assert_eq!(choisi, "packaging/docker/alpine/Dockerfile");

    // Et les dependances retenues sont bien celles de l'application.
    let r = parse_dockerfile(&choisi, &charger("miniflux.Dockerfile"));
    assert!(!r.apt_packages.contains(&"devscripts".to_string()));
}
