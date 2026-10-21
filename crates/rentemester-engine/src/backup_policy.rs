//! Pure backup freshness and optional bookkeeping-lock calculation. Inputs are
//! already resolved activity instants, not filenames, downloads or trust claims.
//! Callers retain responsibility for qualifying backups and activity evidence.
use crate::Error;
use serde::Serialize;

const DAY_MS: i64 = 86_400_000;
const WEEK_MS: i64 = 7 * DAY_MS;
// JavaScript Date's inclusive millisecond range, retained for legacy parity.
const MAX_DATE_MS: i64 = 8_640_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub backup_due: bool,
    pub has_activity_since_backup: bool,
    /// Legacy two-decimal day count; may be negative for an as-of query before
    /// the latest backup. Null when no qualified backup exists.
    pub days_since_latest_backup: Option<f64>,
    /// Latest backup plus seven days while activity is outstanding. With no
    /// backup but existing activity, the legacy freshness deadline is `now`.
    pub required_by_ms: Option<i64>,
    pub locked: bool,
    /// Freshness lateness: max(0, now - requiredBy), independent of opt-in lock
    /// enforcement and grace. A never-backed company's requiredBy is now, so
    /// this value is zero even when its separate lock deadline has passed.
    pub overdue_ms: i64,
    /// Present only when enforcement is enabled AND a backup is due. Uses the
    /// latest backup, otherwise earliest activity, plus seven days and grace.
    pub lock_deadline_ms: Option<i64>,
}
fn invalid(message: &str) -> Error {
    Error::new("invalid_backup_policy", message)
}
fn date(value: i64) -> Result<i64, Error> {
    if !(-MAX_DATE_MS..=MAX_DATE_MS).contains(&value) {
        return Err(invalid(
            "timestamp exceeds the supported Date millisecond range",
        ));
    }
    Ok(value)
}
fn deadline(anchor: i64, grace_ms: i64) -> Result<i64, Error> {
    date(
        anchor
            .checked_add(WEEK_MS)
            .and_then(|value| value.checked_add(grace_ms))
            .ok_or_else(|| invalid("backup deadline overflow"))?,
    )
}
// Match Number(days.toFixed(2)): round the exact binary64 quotient, with
// halfway magnitudes rounded up. Multiplying a float by100 before rounding
// can itself round a value onto a tie and disagree with the legacy result.
fn rounded_days(delta: i64) -> f64 {
    if delta == 0 {
        return 0.0;
    }
    let days = delta as f64 / DAY_MS as f64;
    let bits = days.abs().to_bits();
    let significand = u128::from((bits & ((1_u64 << 52) - 1)) | (1_u64 << 52));
    let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023 - 52;
    // Valid Date differences give quotients in [1/dayMs, 200000000], so
    // exponent lies in [-79,-25] and these exact integer operations fit u128.
    let denominator = 1_u128 << (-exponent as u32);
    let scaled = significand * 100;
    let cents = scaled / denominator + u128::from(scaled % denominator >= denominator / 2);
    if cents == 0 {
        return 0.0;
    }
    let rounded = cents as f64 / 100.0;
    if delta < 0 { -rounded } else { rounded }
}

/// Preserve legacy weekly freshness (> seven days) and lock (>= deadline)
/// boundaries. A lock is always gated by freshness being due; consequently at
/// exactly seven days after a backup, zero grace still does not engage it.
/// No clock, storage or default policy is read here. Callers choose off/zero
/// defaults explicitly. Future instants remain valid for legacy as-of queries.
pub fn evaluate(
    now_ms: i64,
    latest_backup_ms: Option<i64>,
    latest_activity_ms: Option<i64>,
    earliest_activity_ms: Option<i64>,
    enforced: bool,
    grace_days: u32,
) -> Result<Status, Error> {
    evaluate_observed(now_ms,latest_backup_ms,latest_activity_ms,earliest_activity_ms,enforced,grace_days,false)
}
/// A host can additionally prove new append-only activity identities even when
/// coarse source timestamps fall in the same second as the stored snapshot.
pub fn evaluate_observed(now_ms:i64,latest_backup_ms:Option<i64>,latest_activity_ms:Option<i64>,earliest_activity_ms:Option<i64>,enforced:bool,grace_days:u32,activity_changed:bool)->Result<Status,Error>{
    date(now_ms)?;
    for timestamp in [latest_backup_ms, latest_activity_ms, earliest_activity_ms]
        .into_iter()
        .flatten()
    {
        date(timestamp)?;
    }
    if earliest_activity_ms
        .zip(latest_activity_ms)
        .is_some_and(|(first, last)| first > last)
    {
        return Err(invalid("earliest activity must not follow latest activity"));
    }
    let grace_ms = i64::from(grace_days)
        .checked_mul(DAY_MS)
        .ok_or_else(|| invalid("backup grace interval overflow"))?;
    let has_activity_since_backup = activity_changed || latest_activity_ms
        .is_some_and(|activity| latest_backup_ms.is_none_or(|backup| activity > backup));
    let elapsed_ms = latest_backup_ms.map(|backup| now_ms - backup);
    let backup_due =
        has_activity_since_backup && elapsed_ms.is_none_or(|elapsed| elapsed > WEEK_MS);
    let required_by_ms = if has_activity_since_backup {
        Some(match latest_backup_ms {
            Some(backup) => deadline(backup, 0)?,
            None => now_ms,
        })
    } else {
        None
    };
    let lock_deadline_ms = if enforced && backup_due {
        Some(match latest_backup_ms.or(earliest_activity_ms) {
            Some(anchor) => deadline(anchor, grace_ms)?,
            // Preserve the defensive legacy fallback when activity exists but
            // its earliest instant is unavailable; do not invent a fresh week.
            None => now_ms,
        })
    } else {
        None
    };
    Ok(Status {
        backup_due,
        has_activity_since_backup,
        days_since_latest_backup: elapsed_ms.map(rounded_days),
        required_by_ms,
        locked: lock_deadline_ms.is_some_and(|deadline| now_ms >= deadline),
        overdue_ms: required_by_ms.map_or(0, |required| (now_ms - required).max(0)),
        lock_deadline_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn no_activity_and_no_new_activity_never_lock() {
        let empty = evaluate(20 * DAY_MS, None, None, None, true, 0).unwrap();
        assert!(!empty.backup_due && !empty.locked);
        assert_eq!(empty.required_by_ms, None);
        assert_eq!(empty.days_since_latest_backup, None);
        for activity in [None, Some(0), Some(DAY_MS)] {
            let status = evaluate(20 * DAY_MS, Some(DAY_MS), activity, Some(0), true, 0).unwrap();
            assert!(!status.has_activity_since_backup && !status.backup_due && !status.locked);
            assert_eq!(status.lock_deadline_ms, None);
            assert_eq!(status.overdue_ms, 0);
        }
    }
    #[test]
    fn weekly_due_is_strict_but_due_lock_deadline_is_inclusive() {
        let exact = evaluate(WEEK_MS, Some(0), Some(1), Some(1), true, 0).unwrap();
        assert!(!exact.backup_due && !exact.locked);
        assert_eq!(exact.required_by_ms, Some(WEEK_MS));
        assert_eq!(exact.lock_deadline_ms, None);
        let next = evaluate(WEEK_MS + 1, Some(0), Some(1), Some(1), true, 0).unwrap();
        assert!(next.backup_due && next.locked);
        assert_eq!(next.lock_deadline_ms, Some(WEEK_MS));
        assert_eq!(next.overdue_ms, 1);
        let grace = evaluate(9 * DAY_MS - 1, Some(0), Some(1), Some(1), true, 2).unwrap();
        assert!(grace.backup_due && !grace.locked);
        assert_eq!(grace.lock_deadline_ms, Some(9 * DAY_MS));
        assert!(
            evaluate(9 * DAY_MS, Some(0), Some(1), Some(1), true, 2)
                .unwrap()
                .locked
        );
    }
    #[test]
    fn never_backed_due_immediately_but_lock_anchors_to_first_activity() {
        let young = evaluate(DAY_MS, None, Some(DAY_MS), Some(0), true, 1).unwrap();
        assert!(young.backup_due && !young.locked);
        assert_eq!(young.required_by_ms, Some(DAY_MS));
        assert_eq!(young.lock_deadline_ms, Some(8 * DAY_MS));
        let old = evaluate(8 * DAY_MS, None, Some(2 * DAY_MS), Some(0), true, 1).unwrap();
        assert!(old.locked);
        assert_eq!(old.overdue_ms, 0);
        let missing_anchor = evaluate(DAY_MS, None, Some(DAY_MS), None, true, 9).unwrap();
        assert!(missing_anchor.locked);
        assert_eq!(missing_anchor.lock_deadline_ms, Some(DAY_MS));
        let off = evaluate(20 * DAY_MS, Some(0), Some(1), Some(1), false, 0).unwrap();
        assert!(off.backup_due && !off.locked);
        assert_eq!(off.lock_deadline_ms, None);
        assert_eq!(off.overdue_ms, 13 * DAY_MS);
    }
    #[test]
    fn day_rounding_and_as_of_dates_follow_legacy_binary64_semantics() {
        for (delta, expected) in [
            (1, 0.0),
            (DAY_MS / 8, 0.13),
            (3 * DAY_MS / 8, 0.38),
            (1005 * DAY_MS / 1000, 1.0),
            (-DAY_MS / 8, -0.13),
        ] {
            assert_eq!(rounded_days(delta), expected);
        }
        let future = evaluate(0, Some(DAY_MS), Some(DAY_MS), Some(0), true, 0).unwrap();
        assert_eq!(future.days_since_latest_backup, Some(-1.0));
        assert!(!future.backup_due && !future.locked);
        let serialized = serde_json::to_value(future).unwrap();
        assert!(serialized.get("backupDue").is_some());
        assert!(serialized.get("lockDeadlineMs").is_some());
        assert!(serialized.get("backup_due").is_none());
    }
    #[test]
    fn invalid_timestamps_activity_ranges_and_deadlines_refuse() {
        assert!(evaluate(i64::MAX, None, None, None, false, 0).is_err());
        assert!(evaluate(0, Some(i64::MIN), None, None, false, 0).is_err());
        assert!(evaluate(0, None, Some(1), Some(2), true, 0).is_err());
        assert!(
            evaluate(
                MAX_DATE_MS,
                Some(MAX_DATE_MS - 1),
                Some(MAX_DATE_MS),
                Some(0),
                true,
                0
            )
            .is_err()
        );
        assert!(evaluate(10 * DAY_MS, None, Some(1), Some(0), true, u32::MAX).is_err());
        assert!(evaluate(-MAX_DATE_MS, None, None, None, false, 0).is_ok());
    }
}

#[cfg(test)]mod activity_identity_tests{use super::*;#[test]fn same_second_identity_change_cannot_suppress_weekly_backup(){let backup=1767225600500;let latest=1767225600000;let now=backup+8*DAY_MS;assert!(!evaluate(now,Some(backup),Some(latest),Some(latest),true,0).unwrap().backup_due);let observed=evaluate_observed(now,Some(backup),Some(latest),Some(latest),true,0,true).unwrap();assert!(observed.backup_due&&observed.locked&&observed.has_activity_since_backup);}}
