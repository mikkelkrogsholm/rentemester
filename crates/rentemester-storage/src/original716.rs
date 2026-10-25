//! Source structure-only projection; no accounting calculations or SQL writes.
use rentemester_engine::intercompany::Manifest;
use crate::workspace_context::Catalog;
use serde_json::{Value, json};
use std::collections::BTreeSet;
const BLOCKERS: [&str; 2] = [
    "structure overview deliberately contains no financial consolidation",
    "use an approved reporting profile and the consolidated-report contract for figures",
];
pub fn group_overview(
    manifest: Option<&Manifest>, catalog: &Catalog, visible: &BTreeSet<String>,
    as_of: &str, available: impl Fn(&str) -> bool,
) -> Value {
    let mut result = json!({"ok":true,"scope":"structure-status-only","consolidationStatus":"not-available","consolidatedFigures":null,"rawCompanySums":null,"blockers":BLOCKERS,"manifestStatus":"not-configured","asOf":as_of,"groups":[]});
    let Some(manifest) = manifest else { return result; };
    let mut groups = vec![];
    let mut any_active = false;
    for group in &manifest.groups {
        let active: Vec<_> = group.memberships.iter().filter(|m| m.interval.active(as_of)).collect();
        any_active |= !active.is_empty();
        let members: Vec<_> = active.iter().filter(|m| visible.contains(&m.company_slug)).map(|m| {
            let mut v = serde_json::to_value(m).expect("membership");
            v["archived"] = json!(catalog.companies.iter().any(|c| c.slug == m.company_slug && c.archived));
            v
        }).collect();
        if members.is_empty() { continue; }
        let partial = members.len() != active.len();
        let ownership: Vec<_> = group.ownership.iter().filter(|e| e.interval.active(as_of)).collect();
        let visible_ownership: Vec<_> = ownership.iter().filter(|e| visible.contains(&e.parent_company_slug) && visible.contains(&e.child_company_slug)).collect();
        let hidden = visible_ownership.len() != ownership.len();
        let archived = active.iter().any(|m| catalog.companies.iter().any(|c| c.slug == m.company_slug && c.archived));
        let missing = active.iter().any(|m| !available(&m.company_slug));
        let mut blockers: Vec<_> = BLOCKERS.iter().copied().collect();
        if partial || hidden { blockers.insert(0, "group structure is partial for this user"); }
        if archived { blockers.insert(0, "one or more active group members are archived"); }
        if missing { blockers.insert(0, "one or more active group members have no available ledger"); }
        let blocked = partial || hidden || archived || missing;
        let mut row = json!({"partial":partial,"visibleMemberships":members,"visibleOwnership":visible_ownership,"readiness":if blocked {"blocked"} else {"ready"},"blockers":blockers});
        if !partial { row["id"] = json!(group.id); row["name"] = json!(group.name); }
        groups.push(row);
    }
    result["manifestStatus"] = json!(if !any_active || groups.iter().any(|g| g["readiness"] == "blocked") { "blocked" } else { "ready" });
    if !any_active { result["blockers"].as_array_mut().unwrap().push(json!("no active group memberships exist at this date")); }
    result["groups"] = json!(groups);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positive_and_missing_ledger_are_distinct_and_date_is_half_open() {
        let catalog = Catalog { workspace:"synthetic".into(), companies:vec![crate::workspace_context::RegisteredCompany { slug:"synthetic-a".into(), name:"Synthetic A".into(), archived:false }] };
        let manifest: Manifest = serde_json::from_value(json!({"version":1,"groups":[{"id":"synthetic","name":"Synthetic","memberships":[{"id":"member","companySlug":"synthetic-a","validFrom":"2026-01-01","validToExclusive":"2027-01-01"}],"ownership":[]}]})).unwrap();
        let visible = BTreeSet::from(["synthetic-a".into()]);
        let ready = group_overview(Some(&manifest), &catalog, &visible, "2026-06-01", |_| true);
        assert_eq!(ready["groups"][0]["readiness"], "ready");
        assert!(ready["consolidatedFigures"].is_null());
        assert_eq!(group_overview(Some(&manifest), &catalog, &visible, "2026-06-01", |_| false)["groups"][0]["readiness"], "blocked");
        assert_eq!(group_overview(Some(&manifest), &catalog, &visible, "2027-01-01", |_| true)["groups"], json!([]));
        assert_eq!(group_overview(None, &catalog, &visible, "2026-06-01", |_| true)["manifestStatus"], "not-configured");
    }
}

pub mod bootstrap;
pub mod custody;
pub mod report;
pub mod schema;

pub mod transfer;
