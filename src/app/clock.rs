//! Heure locale, pour l'affichage et les noms de fichiers.
//!
//! Le décalage du fuseau est lu une seule fois, au démarrage, par
//! [`init_local_offset`] : sous Unix, `time` refuse de le lire quand
//! d'autres threads existent (la lecture de l'environnement n'y est pas
//! sûre), ce qui est le cas dès que le runtime tokio tourne. Sans
//! initialisation (tests, échec de lecture), l'heure reste en UTC.

use std::sync::OnceLock;
use std::time::SystemTime;

use time::{OffsetDateTime, UtcOffset};

static LOCAL_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

/// Lit le décalage du fuseau local. À appeler dans `main`, avant de
/// démarrer le runtime multi-thread.
pub fn init_local_offset() {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    let _ = LOCAL_OFFSET.set(offset);
}

fn local(time: SystemTime) -> OffsetDateTime {
    let offset = LOCAL_OFFSET.get().copied().unwrap_or(UtcOffset::UTC);
    OffsetDateTime::from(time).to_offset(offset)
}

/// Date et heure courtes pour l'historique : `12/10 21:54:20`. La date
/// compte depuis que l'historique survit d'un jour à l'autre.
pub fn short_date_time(time: SystemTime) -> String {
    let t = local(time);
    format!(
        "{:02}/{:02} {:02}:{:02}:{:02}",
        t.day(),
        u8::from(t.month()),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// Horodatage pour un nom de fichier : `20261012-215420`.
pub fn file_stamp(time: SystemTime) -> String {
    let t = local(time);
    format!(
        "{:04}{:02}{:02}-{:02}{:02}{:02}",
        t.year(),
        u8::from(t.month()),
        t.day(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    // Sans `init_local_offset`, les tests tournent en UTC : résultats stables.
    const SAMPLE: u64 = 1_791_755_660; // 2026-10-11 21:54:20 UTC

    #[test]
    fn short_date_time_shows_day_month_and_time() {
        let time = UNIX_EPOCH + Duration::from_secs(SAMPLE);
        assert_eq!(short_date_time(time), "11/10 21:54:20");
    }

    #[test]
    fn file_stamp_is_sortable() {
        let time = UNIX_EPOCH + Duration::from_secs(SAMPLE);
        assert_eq!(file_stamp(time), "20261011-215420");
    }
}
