//! Vitalite du projet amont.
//!
//! Le niveau 8 du catalogue YunoHost exige une maintenance effective. Packager
//! un projet abandonne cree une dette pour la communaute : autant le savoir
//! avant d'y passer du temps.

use ynp_core::facts::RepoMeta;

/// Seuil d'inactivite, en jours, au-dela duquel un depot est considere a risque.
/// Deux ans : assez pour distinguer un projet stable d'un projet abandonne.
pub const INACTIVITY_DAYS: i64 = 730;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaintenanceRisk {
    Healthy,
    /// Sans commit depuis plus de [`INACTIVITY_DAYS`] jours.
    Stale,
    /// Archive par son auteur : il n'y aura pas de correctif de securite.
    Archived,
    /// Date du dernier push inconnue.
    Unknown,
}

pub fn assess(meta: &RepoMeta, today: &str) -> MaintenanceRisk {
    if meta.archived {
        return MaintenanceRisk::Archived;
    }
    match meta
        .pushed_at
        .as_deref()
        .and_then(|d| days_between(d, today))
    {
        None => MaintenanceRisk::Unknown,
        Some(d) if d > INACTIVITY_DAYS => MaintenanceRisk::Stale,
        Some(_) => MaintenanceRisk::Healthy,
    }
}

/// Nombre de jours entre deux dates ISO-8601.
///
/// Un calendrier complet serait disproportionne : on compare un seuil de deux
/// ans, ou une erreur d'un jour sur les annees bissextiles est sans effet.
fn days_between(from: &str, to: &str) -> Option<i64> {
    Some(to_days(to)? - to_days(from)?)
}

fn to_days(date: &str) -> Option<i64> {
    let d = date.get(..10)?;
    let mut parts = d.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Formule de Howard Hinnant : jours depuis une epoque civile.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(archived: bool, pushed: Option<&str>) -> RepoMeta {
        RepoMeta {
            archived,
            pushed_at: pushed.map(str::to_string),
            ..Default::default()
        }
    }

    const TODAY: &str = "2026-09-23";

    #[test]
    fn un_depot_actif_est_sain() {
        assert_eq!(
            assess(&meta(false, Some("2026-08-01T10:00:00Z")), TODAY),
            MaintenanceRisk::Healthy
        );
    }

    #[test]
    fn un_depot_archive_l_emporte_sur_toute_autre_consideration() {
        // Meme pousse hier : archive veut dire pas de correctif de securite.
        assert_eq!(
            assess(&meta(true, Some("2026-09-22")), TODAY),
            MaintenanceRisk::Archived
        );
    }

    #[test]
    fn deux_ans_sans_commit_signalent_un_abandon() {
        assert_eq!(
            assess(&meta(false, Some("2024-01-15")), TODAY),
            MaintenanceRisk::Stale
        );
        // Juste en deca du seuil : encore sain.
        assert_eq!(
            assess(&meta(false, Some("2024-10-01")), TODAY),
            MaintenanceRisk::Healthy
        );
    }

    #[test]
    fn une_date_absente_ou_illisible_ne_conclut_rien() {
        assert_eq!(assess(&meta(false, None), TODAY), MaintenanceRisk::Unknown);
        assert_eq!(
            assess(&meta(false, Some("pas une date")), TODAY),
            MaintenanceRisk::Unknown
        );
        assert_eq!(
            assess(&meta(false, Some("2026-13-45")), TODAY),
            MaintenanceRisk::Unknown
        );
    }

    #[test]
    fn le_calcul_de_jours_traverse_les_annees_bissextiles() {
        assert_eq!(
            days_between("2024-02-28", "2024-03-01"),
            Some(2),
            "2024 est bissextile"
        );
        assert_eq!(days_between("2023-02-28", "2023-03-01"), Some(1));
        assert_eq!(days_between("2024-01-01", "2026-01-01"), Some(731));
    }
}
