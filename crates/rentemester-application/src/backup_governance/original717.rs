//! Source projections, with exact retained-byte joins rather than bare placement declarations.
use super::*;
pub fn data(
    c: &rusqlite::Transaction<'_>,
    l: &rusqlite::Transaction<'_>,
    workspace: &str,
    slug: &str,
    clock: &dyn rentemester_runtime::auth::Clock,
    as_of: Option<String>,
) -> Result<Value, Error> {
    let now = clock.now_unix_ms().map_err(|_| invalid())?;
    let observed = as_of
        .as_deref()
        .map(rentemester_storage::backup_state::original122_moment)
        .transpose();
    let compliance =
        crate::backup_original122::status(c, l, workspace, slug, clock, as_of.clone())?;
    let ds = crate::backup_registration::original717::destinations(c, l, slug)?;
    let compliant = |d: &Value| {
        d["regionAttestation"]["inEeaOrEu"] == true
            && d["nonRelatedParty"] == true
            && d["itSecurityAttestation"]["meetsRecognisedStandards"] == true
    };
    let count = ds.iter().filter(|d| compliant(d)).count();
    let mut placed = 0;
    let mut offsite = false;
    let mut lock = json!({"enforced":false,"locked":false,"backupDue":false,"graceDays":0,"latestBackupAt":compliance["latestBackupAt"],"daysSinceLatestBackup":compliance["daysSinceLatestBackup"],"lockAt":null,"checkedAt":compliance["checkedAt"],"reason":"backup status could not be evaluated","errors":compliance["errors"]});
    let settings = rentemester_storage::backup_state::settings(c, l)?;
    lock["enforced"] = json!(settings.enforced);
    lock["graceDays"] = json!(settings.grace_days);
    if let Ok(at) = observed {
        let at = at.unwrap_or(now);
        let native = crate::backup_status::data_at(c, l, workspace, slug, at, true)?;
        lock["locked"] = native["lock"]["locked"].clone();
        lock["backupDue"] = compliance["backupDue"].clone();
        lock["lockAt"] = native["lock"]["lockDeadline"].clone();
        lock["reason"] = json!(if !settings.enforced {
            "backup lock is not enforced (opt-in)"
        } else if native["lock"]["locked"] == true {
            "bookkeeping is locked: a weekly backup (BEK 205/2024 §4) is overdue past the grace window"
        } else if compliance["backupDue"] == true {
            "a backup is due — bookkeeping will lock when the grace window lapses"
        } else if compliance["hasActivitySinceBackup"] == true {
            "backup is recent enough"
        } else {
            "no bookkeeping activity since the last backup — no backup is due"
        });
        let retained = crate::backup_status::original717::retained(c, l, slug);
        if let Ok(backups) = retained {
            if let Some(b) = backups
                .iter()
                .find(|b| compliance["latestBackupId"] == b.id)
            {
                for d in &ds {
                    for p in d["placements"].as_array().ok_or_else(invalid)? {
                        if p["backupId"] != b.id {
                            continue;
                        }
                        let Some(time) = p["placedAt"].as_str().and_then(|s| {
                            rentemester_storage::backup_state::original122_moment(s).ok()
                        }) else {
                            continue;
                        };
                        if time > at {
                            continue;
                        }
                        if p["verifyMethod"] == "remote-provider"
                            && !p["remoteEvidence"]["verifiedAt"]
                                .as_str()
                                .and_then(|s| {
                                    rentemester_storage::backup_state::original122_moment(s).ok()
                                })
                                .is_some_and(|t| t <= at)
                        {
                            continue;
                        }
                        placed += 1;
                        if p["verified"] == true
                            && p["archiveSha256"] == b.hash
                            && p["archiveSizeBytes"] == b.size
                            && compliant(d)
                        {
                            offsite = true;
                        }
                    }
                }
            }
        }
    }
    Ok(
        json!({"ok":compliance["errors"].as_array().is_some_and(Vec::is_empty)&&compliance["backupDue"]==false&&(compliance["latestBackupId"].is_null()||offsite),"appliedRules":["DK-BOOKKEEPING-BACKUP-001"],"compliance":compliance,"lock":lock,"destinationCount":ds.len(),"compliantDestinationCount":count,"hasCompliantDestination":count>0,"latestBackupPlacedOffsite":offsite,"latestBackupPlacementCount":placed,"checkedAt":compliance["checkedAt"],"errors":compliance["errors"],"destinations":ds}),
    )
}
