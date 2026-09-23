//! Gates G0 et G1.
//!
//! L'ordre des portes suit le cout croissant : on refuse pour une question de
//! licence avant de depenser trente minutes de CPU. G0 se verifie sur les
//! seules metadonnees de la forge, G1 sur l'analyse complete.

use crate::{assess, Rule};
use ynp_core::facts::RepoFacts;
use ynp_core::finding::{Feasibility, Severity};
use ynp_core::gate::GateOutcome;

/// G0 — legal et policy. Ne fait intervenir que les regles peu couteuses.
pub fn g0(facts: &RepoFacts) -> GateOutcome {
    let regles: Vec<Box<dyn Rule>> = vec![
        Box::new(crate::licence::NonLibre),
        Box::new(crate::licence::Abandonne),
    ];

    let bloquants: Vec<_> = regles
        .iter()
        .filter_map(|r| r.check(facts))
        .filter(|f| f.severity == Severity::Blocker)
        .collect();

    if bloquants.is_empty() {
        GateOutcome::Pass
    } else {
        GateOutcome::fail(bloquants)
    }
}

/// G1 — faisabilite. Un seul constat bloquant suffit a arreter le pipeline.
///
/// Le score ne decide pas : il ne separe que « faisable » de « faisable avec
/// travail ». Une application au score bas mais sans bloquant reste packageable,
/// au prix d'arbitrages a porter dans l'appspec.
pub fn g1(feasibility: &Feasibility) -> GateOutcome {
    let bloquants: Vec<_> = feasibility.blockers().cloned().collect();
    if bloquants.is_empty() {
        GateOutcome::Pass
    } else {
        GateOutcome::fail(bloquants)
    }
}

/// Enchaine G0 puis G1 sur un depot analyse.
pub fn evaluate(facts: &RepoFacts, threshold: u8) -> (Feasibility, ynp_core::gate::GateReport) {
    use ynp_core::gate::{GateId, GateReport};

    let feasibility = assess(facts, threshold);
    let mut report = GateReport::default();

    if report.record(GateId::G0Policy, g0(facts)) {
        report.record(GateId::G1Feasibility, g1(&feasibility));
    }
    (feasibility, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::depot_sain;
    use ynp_core::gate::GateId;

    #[test]
    fn un_depot_sain_passe_les_deux_portes() {
        let (_, report) = evaluate(&depot_sain(), 60);
        assert!(report.all_passed());
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn une_licence_absente_arrete_des_la_premiere_porte() {
        let mut f = depot_sain();
        f.meta.license_spdx = None;

        let (_, report) = evaluate(&f, 60);
        assert_eq!(report.first_failure().unwrap().gate, GateId::G0Policy);
        // G1 n'est pas evaluee : inutile de continuer.
        assert_eq!(report.results.len(), 1);
        assert_eq!(report.exit_code(), 10);
    }

    #[test]
    fn un_blocage_de_faisabilite_arrete_a_la_seconde_porte() {
        let mut f = depot_sain();
        f.services.requires_container_runtime = true;

        let (_, report) = evaluate(&f, 60);
        assert_eq!(report.first_failure().unwrap().gate, GateId::G1Feasibility);
        assert_eq!(report.exit_code(), 11);
    }

    #[test]
    fn un_score_bas_sans_bloquant_laisse_passer() {
        // « Faisable avec travail » n'est pas un refus : les arbitrages se
        // portent dans l'appspec.
        let mut f = depot_sain();
        f.stack = Default::default(); // STACK001, majeur
        f.releases.clear(); // SRC002, majeur
        f.meta.archived = true; // MAINT001, majeur

        let (feasibility, report) = evaluate(&f, 60);
        assert!(feasibility.score < 60, "score : {}", feasibility.score);
        assert!(report.all_passed(), "aucun bloquant : le pipeline continue");
    }
}
