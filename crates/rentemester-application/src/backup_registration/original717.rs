//! Flat source contracts backed by immutable native registrations and company placement history.
use super::*;
use serde::Deserialize;
const RULE: &str = "DK-BOOKKEEPING-BACKUP-001";
fn refusal(message: &str) -> Value {
    json!({"ok":false,"appliedRules":[RULE],"errors":[message]})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Add {
    pub label: String,
    pub kind: String,
    pub location: String,
    pub attested_by: String,
    pub in_eea_or_eu: bool,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub non_related_party: Option<bool>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub region_country: Option<String>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub region_note: Option<String>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub it_security_meets_standards: Option<bool>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub it_security_note: Option<String>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub at: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remove {
    pub id: String,
    #[serde(default)]
    pub confirm: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Confirm {
    pub destination_id: String,
    pub backup_id: String,
    pub archive_sha256: String,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub archive_size_bytes: Option<f64>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub actor_kind: Option<String>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub at: Option<String>,
    #[serde(default, deserialize_with = "crate::backup_original122::present")]
    pub note: Option<String>,
    #[serde(default)]
    pub confirm: bool,
}
pub enum Operation {
    List,
    Add(Add),
    Remove(Remove),
    Confirm(Confirm),
    Governance(Option<String>),
}
impl Operation {
    pub fn confirmed(&self) -> bool {
        match self {
            Self::Add(i) => i.confirm,
            Self::Remove(i) => i.confirm,
            Self::Confirm(i) => i.confirm,
            _ => true,
        }
    }
}
pub fn destinations(
    c: &Transaction<'_>,
    l: &Transaction<'_>,
    slug: &str,
) -> Result<Vec<Value>, Error> {
    let (items, history) = store::list(c, slug)?;
    let mut out = vec![];
    for d in items {
        let id = d["destinationId"].as_str().ok_or_else(refused)?;
        let mut placements = rentemester_storage::backup_state::original717::placements(l, id)?;
        placements.extend(
            history
                .iter()
                .filter(|r| r.action == "declare" && r.destination_id == id)
                .map(|r| r.result.clone()),
        );
        out.push(json!({"id":id,"label":d["label"],"kind":d["kind"],"location":d["location"],"nonRelatedParty":d["nonRelatedParty"],"regionAttestation":{"inEeaOrEu":d["inEeaOrEu"],"country":d["regionCountry"],"attestedBy":d["attestedBy"],"attestedAt":d["createdAt"],"note":d["regionNote"]},"itSecurityAttestation":if d["itSecurityMeetsStandards"].is_boolean(){json!({"meetsRecognisedStandards":d["itSecurityMeetsStandards"],"attestedBy":d["attestedBy"],"attestedAt":d["createdAt"],"note":d["itSecurityNote"]})}else{Value::Null},"createdAt":d["createdAt"],"createdBy":d["createdBy"],"placements":placements}));
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(out)
}
fn receipt_id() -> Result<String, Error> {
    Ok(format!(
        "source-{}",
        rentemester_runtime::backup_artifact::original717::sha256(
            rentemester_runtime::auth::new_token()
                .map_err(|_| refused())?
                .as_str()
                .as_bytes()
        )[..32]
            .to_owned()
    ))
}
pub fn apply(
    c: &Transaction<'_>,
    l: &Transaction<'_>,
    workspace: &str,
    slug: &str,
    clock: &dyn rentemester_runtime::auth::Clock,
    subject: &str,
    configs: &[Config],
    op: Operation,
) -> Result<Value, Error> {
    if !op.confirmed() {
        return Ok(
            json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool"],"appliedRules":[RULE]}),
        );
    }
    let now = clock.now_unix_ms().map_err(|_| refused())?;
    let stamp = |at: Option<&str>| -> Result<String, Error> {
        rentemester_runtime::backup_keys::timestamp(match at {
            Some(s) => rentemester_storage::backup_state::original122_moment(s)?,
            None => now,
        })
        .map_err(|_| refused())
    };
    let actor = format!("user:{subject}");
    match op {
        Operation::List => {
            let ds = destinations(c, l, slug)?;
            Ok(json!({"ok":true,"destinationCount":ds.len(),"destinations":ds}))
        }
        Operation::Governance(as_of) => {
            crate::backup_governance::original717::data(c, l, workspace, slug, clock, as_of)
        }
        Operation::Add(i) => {
            let at = match stamp(i.at.as_deref()) {
                Ok(s) => s,
                Err(_) => {
                    return Ok(refusal(
                        "at must be a valid ISO-8601 datetime when provided",
                    ));
                }
            };
            let label = i.label.trim();
            let location = i.location.trim();
            if label.is_empty() {
                return Ok(refusal("label is required"));
            }
            if location.is_empty() {
                return Ok(refusal("location is required"));
            }
            if i.attested_by.trim().is_empty() {
                return Ok(refusal(
                    "attestedBy is required — a human must attest where this backup is stored",
                ));
            }
            let id = format!(
                "dest-{}",
                &rentemester_runtime::backup_artifact::original717::sha256(
                    format!("{label}|{location}|{at}").as_bytes()
                )[..12]
            );
            let target = configs
                .iter()
                .find(|cfg| cfg.kind == i.kind && cfg.directory.to_str() == Some(location))
                .map(|cfg| cfg.id.clone());
            let request = Request::Add {
                operation_id: receipt_id()?,
                destination_id: id.clone(),
                label: label.into(),
                kind: i.kind,
                location: location.into(),
                attested_by: i.attested_by,
                in_eea_or_eu: i.in_eea_or_eu,
                non_related_party: i.non_related_party.unwrap_or(true),
                region_country: i.region_country,
                region_note: i.region_note,
                it_security_meets_standards: i.it_security_meets_standards,
                it_security_note: i.it_security_note,
                operator_target_id: target,
                confirm: true,
            };
            if request.canonical().is_err() {
                return Ok(refusal("destination fields are invalid"));
            }
            if store::list(c, slug)?
                .1
                .iter()
                .any(|r| r.destination_id == id)
            {
                return Ok(refusal(&format!(
                    "a destination with the same label, location and timestamp already exists: {id}"
                )));
            }
            store::apply(c, slug, &request, &actor, &at, None)?;
            let d = destinations(c, l, slug)?
                .into_iter()
                .find(|d| d["id"] == id)
                .ok_or_else(refused)?;
            Ok(
                json!({"ok":true,"destination":d,"appliedRules":[RULE,"DK-BOOKKEEPING-BACKUP-DEST-001"],"errors":[]}),
            )
        }
        Operation::Remove(i) => {
            if !store::list(c, slug)?
                .0
                .iter()
                .any(|d| d["destinationId"] == i.id)
            {
                return Ok(refusal(&format!("no backup destination with id: {}", i.id)));
            }
            store::apply(
                c,
                slug,
                &Request::Remove {
                    operation_id: receipt_id()?,
                    destination_id: i.id,
                    confirm: true,
                },
                &actor,
                &stamp(None)?,
                None,
            )?;
            Ok(json!({"ok":true,"appliedRules":[RULE],"errors":[]}))
        }
        Operation::Confirm(i) => {
            if i.backup_id.trim().is_empty() {
                return Ok(refusal("backupId is required"));
            }
            if i.archive_sha256.len() != 64
                || !i.archive_sha256.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Ok(refusal(
                    "archiveSha256 must be a 64-character hex sha256 digest",
                ));
            }
            let at = match stamp(i.at.as_deref()) {
                Ok(s) => s,
                Err(_) => {
                    return Ok(refusal(
                        "at must be a valid ISO-8601 datetime when provided",
                    ));
                }
            };
            let ds = store::list(c, slug)?.0;
            let Some(d) = ds.iter().find(|d| d["destinationId"] == i.destination_id) else {
                return Ok(refusal(&format!(
                    "no backup destination with id: {}",
                    i.destination_id
                )));
            };
            let mut verified = false;
            if let Some(cfg) = configs.iter().find(|cfg| d["operatorTargetId"] == cfg.id) {
                let backups = crate::backup_status::original717::retained(c, l, slug)?;
                let Some(b) = backups
                    .iter()
                    .find(|b| b.id == i.backup_id && b.hash == i.archive_sha256.to_lowercase())
                else {
                    return Ok(refusal("canonical backup does not match declared archive"));
                };
                verified = rentemester_runtime::backup_destination::verify_original717_placement(
                    cfg, &b.id, &b.bytes,
                )
                .map_err(|_| refused())?;
            }
            let p = json!({"backupId":i.backup_id.trim(),"archiveSha256":i.archive_sha256.to_lowercase(),"archiveSizeBytes":i.archive_size_bytes.filter(|n|n.is_finite()&&*n>=0.0).map(f64::trunc),"placedAt":at,"actor":actor,"actorKind":i.actor_kind.as_deref().filter(|s|*s=="human").unwrap_or("agent"),"verified":verified,"verifyMethod":if verified{"sha256-reread"}else{"declared"},"note":i.note.as_deref().map(str::trim).filter(|s|!s.is_empty())});
            rentemester_storage::backup_state::original717::placement(
                l,
                &i.destination_id,
                &p,
                &actor,
                &stamp(None)?,
            )?;
            Ok(json!({"ok":true,"placement":p,"appliedRules":[RULE],"errors":[]}))
        }
    }
}
pub fn human(
    c: &mut Connection,
    l: &mut Connection,
    ctx: Context<'_>,
    configs: &[Config],
    op: Operation,
) -> Result<Committed<Value>, Failure> {
    let read = matches!(op, Operation::List | Operation::Governance(_));
    let workspace = ctx.selection.workspace;
    let slug = ctx.selection.slug;
    let clock = ctx.clock;
    control_fence::execute_cookie_both(
        c,
        l,
        CookieAuthorization {
            cookie: ctx.cookie,
            clock,
            requirement: SessionRequirement::Current,
        },
        ctx.selection,
        if read {
            Permission::CompanyRead
        } else {
            Permission::CompanyAdmin
        },
        read,
        true,
        |c, l, s| apply(c, l, workspace, slug, clock, s, configs, op),
    )
}
pub fn service(
    c: &mut Connection,
    l: &mut Connection,
    ctx: crate::service_company::Context<'_>,
    configs: &[Config],
    op: Operation,
) -> Result<crate::service_company::ServiceResult<Value>, crate::service_company::ServiceFailure> {
    let read = matches!(op, Operation::List | Operation::Governance(_));
    let workspace = ctx.selection.workspace;
    let slug = ctx.selection.slug;
    let clock = ctx.clock;
    let work = |c: &Transaction<'_>, l: &Transaction<'_>, s: &str, _: &mut Option<Value>| {
        Ok(crate::service_company::OperationResult {
            result: apply(c, l, workspace, slug, clock, s, configs, op)?,
            receipt: None,
        })
    };
    if read {
        crate::service_company::execute_operation_both(
            c,
            l,
            ctx,
            Permission::CompanyRead,
            false,
            work,
        )
    } else {
        crate::service_company::execute_recovery_both(c, l, ctx, Permission::CompanyAdmin, work)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixed;
    impl rentemester_runtime::auth::Clock for Fixed {
        fn now_unix_ms(&self) -> Result<i64, rentemester_runtime::auth::AuthError> {
            Ok(1767225600000)
        }
    }
    #[test]
    fn original717_backup_registration_flat_happy_confirmation_tombstone_and_placement_readback() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic backup views")
            .unwrap();
        let c = c.transaction().unwrap();
        let l = l.transaction().unwrap();
        let make = |confirm| {
            serde_json::from_value::<Add>(json!({"label":"Synthetic external copy","kind":"other","location":"opaque:test","attestedBy":"Synthetic operator","inEeaOrEu":true,"nonRelatedParty":true,"itSecurityMeetsStandards":true,"confirm":confirm})).unwrap()
        };
        let invoke = |op| apply(&c, &l, "synthetic", "a", &Fixed, "owner", &[], op).unwrap();
        assert_eq!(
            invoke(Operation::Add(make(false)))["code"],
            "CONFIRM_REQUIRED"
        );
        assert!(destinations(&c, &l, "a").unwrap().is_empty());
        let added = invoke(Operation::Add(make(true)));
        assert_eq!(added["ok"], true);
        let id = added["destination"]["id"].as_str().unwrap().to_owned();
        assert_eq!(added["destination"]["regionAttestation"]["inEeaOrEu"], true);
        assert_eq!(invoke(Operation::List)["destinationCount"], 1);
        let confirm=serde_json::from_value::<Confirm>(json!({"destinationId":id,"backupId":"backup-synthetic","archiveSha256":"a".repeat(64),"confirm":true})).unwrap();
        let declared = invoke(Operation::Confirm(confirm));
        assert_eq!(declared["placement"]["verified"], false);
        assert_eq!(
            invoke(Operation::List)["destinations"][0]["placements"][0],
            declared["placement"]
        );
        assert!(destinations(&c, &l, "other").unwrap().is_empty());
        assert_eq!(
            invoke(Operation::Remove(Remove {
                id: id.clone(),
                confirm: true
            }))["ok"],
            true
        );
        assert_eq!(invoke(Operation::List)["destinationCount"], 0);
        assert_eq!(invoke(Operation::Add(make(true)))["ok"], false); // immutable tombstone prevents reusing history
        assert!(serde_json::from_value::<Remove>(json!({"id":id,"confirm":"true"})).is_err());
        assert!(serde_json::from_value::<Remove>(json!({"id":"synthetic"})).is_ok());
    }
    #[test]
    fn original717_backup_governance_asof_shape_and_unverified_declaration() {
        let mut c = Connection::open_in_memory().unwrap();
        let mut l = Connection::open_in_memory().unwrap();
        rentemester_storage::fresh::initialize_empty_pair(&mut c, &mut l, "Synthetic governance")
            .unwrap();
        let c = c.transaction().unwrap();
        let l = l.transaction().unwrap();
        let result = apply(
            &c,
            &l,
            "w",
            "a",
            &Fixed,
            "owner",
            &[],
            Operation::Governance(Some("invalid".into())),
        )
        .unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["compliance"]["checkedAt"], "invalid");
        assert_eq!(result["latestBackupPlacedOffsite"], false);
        let result = apply(
            &c,
            &l,
            "w",
            "a",
            &Fixed,
            "owner",
            &[],
            Operation::Governance(None),
        )
        .unwrap();
        assert_eq!(result["destinationCount"], 0);
        assert_eq!(result["compliance"]["latestBackupId"], Value::Null);
        assert_eq!(result["lock"]["enforced"], false);
    }
}
