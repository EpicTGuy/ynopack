//! Les six portes de validation.
//!
//! « Une fois toutes les etapes de validation validees, il le package » : chaque
//! etape est une porte qui rend Pass, Fail ou Skipped. Le pipeline s'arrete a la
//! premiere qui echoue, sauf `--force`.

use crate::finding::Finding;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateId {
    /// Licence libre, pas de crypto, depot vivant. Se verifie sans rien telecharger.
    G0Policy,
    /// Aucun bloquant dans le rapport de faisabilite.
    G1Feasibility,
    /// manifest valide vs schema officiel, linter propre, bash -n, aucun FIXME restant.
    G2Static,
    /// Install reelle sur l'hote YunoHost : endpoint HTTP, backup/restore, remove sans residu.
    G3Install,
    /// package_check niveau >= 4, en VM isolee. Requis seulement pour le catalogue officiel.
    G4Quality,
    /// Pousse sur la forge, entree de catalogue generee, reinstall depuis le catalogue.
    G5Publish,
}

impl GateId {
    pub fn code(self) -> &'static str {
        match self {
            GateId::G0Policy => "G0",
            GateId::G1Feasibility => "G1",
            GateId::G2Static => "G2",
            GateId::G3Install => "G3",
            GateId::G4Quality => "G4",
            GateId::G5Publish => "G5",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GateId::G0Policy => "Legal & policy",
            GateId::G1Feasibility => "Faisabilite",
            GateId::G2Static => "Conformite statique",
            GateId::G3Install => "Installabilite",
            GateId::G4Quality => "Qualite (package_check)",
            GateId::G5Publish => "Publication",
        }
    }

    /// Ordre canonique du pipeline.
    pub fn all() -> [GateId; 6] {
        [
            GateId::G0Policy,
            GateId::G1Feasibility,
            GateId::G2Static,
            GateId::G3Install,
            GateId::G4Quality,
            GateId::G5Publish,
        ]
    }

    /// G4 exige une VM Incus dediee ; le circuit Forgejo interne s'en passe.
    pub fn is_optional(self) -> bool {
        matches!(self, GateId::G4Quality)
    }
}

impl fmt::Display for GateId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code(), self.label())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum GateOutcome {
    Pass,
    Fail {
        findings: Vec<Finding>,
    },
    /// Volontairement non executee (gate optionnelle, ou `--skip`).
    Skipped {
        reason: String,
    },
}

impl GateOutcome {
    pub fn fail(findings: Vec<Finding>) -> Self {
        GateOutcome::Fail { findings }
    }

    pub fn skipped(reason: impl Into<String>) -> Self {
        GateOutcome::Skipped {
            reason: reason.into(),
        }
    }

    /// Une gate sautee ne bloque pas : seul un echec franc arrete le pipeline.
    pub fn blocks_pipeline(&self) -> bool {
        matches!(self, GateOutcome::Fail { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateResult {
    pub gate: GateId,
    #[serde(flatten)]
    pub outcome: GateOutcome,
}

/// Trace complete d'un `yunopack run`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    pub results: Vec<GateResult>,
}

impl GateReport {
    pub fn record(&mut self, gate: GateId, outcome: GateOutcome) -> bool {
        let blocks = outcome.blocks_pipeline();
        self.results.push(GateResult { gate, outcome });
        !blocks
    }

    /// Vrai si aucune gate executee n'a echoue.
    pub fn all_passed(&self) -> bool {
        !self.results.iter().any(|r| r.outcome.blocks_pipeline())
    }

    pub fn first_failure(&self) -> Option<&GateResult> {
        self.results.iter().find(|r| r.outcome.blocks_pipeline())
    }

    /// Code de sortie du CLI : 0 si tout passe, sinon le rang de la gate fautive.
    /// Un script appelant sait ainsi *ou* ca a casse sans parser la sortie.
    pub fn exit_code(&self) -> i32 {
        match self.first_failure() {
            None => 0,
            Some(r) => GateId::all()
                .iter()
                .position(|g| *g == r.gate)
                .map_or(1, |i| i as i32 + 10),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::finding::{Finding, Severity};

    #[test]
    fn le_pipeline_continue_tant_que_les_gates_passent() {
        let mut r = GateReport::default();
        assert!(r.record(GateId::G0Policy, GateOutcome::Pass));
        assert!(r.record(GateId::G1Feasibility, GateOutcome::Pass));
        assert!(r.all_passed());
        assert_eq!(r.exit_code(), 0);
    }

    #[test]
    fn une_gate_sautee_ne_bloque_pas() {
        let mut r = GateReport::default();
        assert!(r.record(GateId::G4Quality, GateOutcome::skipped("pas de VM Incus")));
        assert!(r.all_passed());
    }

    #[test]
    fn le_code_de_sortie_designe_la_gate_fautive() {
        let mut r = GateReport::default();
        r.record(GateId::G0Policy, GateOutcome::Pass);
        assert!(!r.record(
            GateId::G2Static,
            GateOutcome::fail(vec![Finding::new("X", Severity::Blocker, "t")])
        ));
        assert_eq!(r.first_failure().unwrap().gate, GateId::G2Static);
        assert_eq!(r.exit_code(), 12);
    }
}
