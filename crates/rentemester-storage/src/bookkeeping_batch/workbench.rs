//! Source-shaped filtered workbench over the SAME complete native plan/universe.
use super::*;
use wire::WorkbenchInput;
const STATUSES: [&str; 9] = [
    "ready",
    "suggestedMatch",
    "missingDocument",
    "partyUnresolved",
    "accountingDecisionRequired",
    "vatEvidenceRequired",
    "dimensionEvidenceRequired",
    "stalePlan",
    "applyFailed",
];
pub fn read(
    tx: &Transaction<'_>,
    input: &WorkbenchInput,
    latest: Date,
    today: &str,
    period_close: Value,
) -> Result<Value, Error> {
    crate::document_party_links::validate_backup(tx)?;
    input.validate()?;
    if input
        .status
        .as_deref()
        .is_some_and(|s| !STATUSES.contains(&s))
    {
        return Err(fail("invalid workbench status"));
    }
    let plan = plan(tx, &input.scope(), latest, today)?;
    let banks = plan["sourceIdentities"]["candidateUniverse"]["banks"]
        .as_array()
        .unwrap();
    let mut population = vec![];
    let mut counts = serde_json::Map::new();
    for s in STATUSES {
        counts.insert(s.into(), json!(0));
    }
    for item in plan["items"].as_array().unwrap() {
        let bank = banks
            .iter()
            .find(|b| b["id"] == item["bankTransactionId"])
            .ok_or_else(|| fail("incomplete plan source population"))?;
        let rule = &item["detail"]["rule"];
        let dims=rule["outcome"]["dimensions"].as_object().map(|m|m.iter().map(|(d,m)|{let active=plan["sourceIdentities"]["candidateUniverse"]["dimensions"].as_array().unwrap().iter().find(|r|r["dimension_id"]==*d&&r["member_id"]==*m);json!({"dimensionId":d,"memberId":m,"status":active.map(|r|r["status"].clone()).unwrap_or(json!("missing"))})}).collect::<Vec<_>>()).unwrap_or_default();
        let mut document = Value::Null;
        let mut status = "missingDocument";
        if let Some(id) = item["documentId"].as_i64() {
            let links = rows(
                tx,
                "SELECT party_id,party_snapshot_json FROM current_document_party_links WHERE document_id=?1 ORDER BY CASE party_role WHEN 'supplier' THEN 0 WHEN 'vendor' THEN 1 WHEN 'issuer' THEN 2 ELSE 3 END,id DESC LIMIT 1",
                [id],
            )?;
            let party = if let Some(link) = links.first() {
                let snapshot: Value = serde_json::from_str(text(link, "party_snapshot_json")?)
                    .map_err(|_| fail("invalid document party snapshot"))?;
                json!({"id":link["party_id"],"name":snapshot["name"].as_str().unwrap_or(text(link,"party_id")?)})
            } else {
                Value::Null
            };
            let internal:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM current_document_party_resolution_events WHERE document_id=?1 AND state='internal_no_external_party')",[id],|r|r.get(0)).map_err(sql)?;
            let resolution = if internal {
                "internal_no_external_party"
            } else if !party.is_null() {
                "resolved"
            } else {
                "unresolved"
            };
            document =
                json!({"id":id,"quality":"matched","party":party,"resolutionState":resolution});
            status = if resolution == "unresolved" {
                "partyUnresolved"
            } else if dims.iter().any(|d| d["status"] != "active") {
                "dimensionEvidenceRequired"
            } else if item["detail"]["vatReady"] == false {
                "vatEvidenceRequired"
            } else if item["partition"] == "ready" {
                "ready"
            } else {
                "accountingDecisionRequired"
            };
        }
        let proposed = json!({"account":rule["outcome"]["account"],"vatTreatment":rule["outcome"]["vatTreatment"],"dimensions":dims,"partyDefaults":null});
        let action = match status {
            "ready" => "Review exact plan, persist and obtain independent approval",
            "missingDocument" => "Attach or review a unique original purchase match",
            "partyUnresolved" => "Resolve the document party explicitly",
            "vatEvidenceRequired" => "Review current native VAT evidence",
            "dimensionEvidenceRequired" => "Review active dimension members",
            _ => "Review explicit purchase rule and accounting evidence",
        };
        *counts.get_mut(status).unwrap() = json!(counts[status].as_u64().unwrap() + 1);
        population.push(json!({"bankTransactionId":item["bankTransactionId"],"date":bank["transaction_date"],"text":bank["text"],"amount":bank["amount"],"currency":bank["currency"],"bankAccount":{"id":bank["bank_account_id"],"name":bank["bank_account_name"]},"document":document,"proposed":proposed,"status":status,"nextAction":action,"sourceHash":item["evidenceHash"],"drilldown":{"bankTransactionId":item["bankTransactionId"],"bankAccountId":bank["bank_account_id"],"documentId":item["documentId"],"partyId":document["party"]["id"],"periodClose":{"from":input.from,"to":input.to}}}));
    }
    let ready = counts["ready"].as_u64().unwrap() as usize;
    let total = population.len();
    let filtered = population
        .into_iter()
        .filter(|r| {
            input.status.as_ref().is_none_or(|s| r["status"] == *s)
                && input
                    .bank_account_id
                    .is_none_or(|n| r["bankAccount"]["id"] == n)
                && input
                    .party_id
                    .as_ref()
                    .is_none_or(|s| r["document"]["party"]["id"] == *s)
                && input.document_quality.as_deref().is_none_or(|s| {
                    if s == "missing" {
                        r["document"].is_null()
                    } else {
                        !r["document"].is_null()
                    }
                })
                && input
                    .account
                    .as_ref()
                    .is_none_or(|s| r["proposed"]["account"] == *s)
                && input
                    .vat_treatment
                    .as_ref()
                    .is_none_or(|s| r["proposed"]["vatTreatment"] == *s)
                && input.dimension.as_ref().is_none_or(|s| {
                    r["proposed"]["dimensions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|d| {
                            format!(
                                "{}:{}",
                                d["dimensionId"].as_str().unwrap(),
                                d["memberId"].as_str().unwrap()
                            ) == *s
                        })
                })
                && input.search.as_ref().is_none_or(|s| {
                    [
                        r["text"].as_str().unwrap_or(""),
                        &r["amount"].to_string(),
                        r["currency"].as_str().unwrap_or(""),
                        r["document"]["party"]["name"].as_str().unwrap_or(""),
                        r["proposed"]["account"].as_str().unwrap_or(""),
                    ]
                    .join(" ")
                    .to_lowercase()
                    .contains(&s.to_lowercase())
                })
        })
        .collect::<Vec<_>>();
    let selection_total = filtered.len();
    let selection_ready = filtered.iter().filter(|r| r["status"] == "ready").count();
    let cursor = input.cursor.unwrap_or(0);
    let limit = input.limit.unwrap_or(25);
    let page = filtered
        .into_iter()
        .skip(cursor)
        .take(limit)
        .collect::<Vec<_>>();
    let next = if cursor.saturating_add(limit) < selection_total {
        Some(cursor + limit)
    } else {
        None
    };
    let cutover: Option<String> = tx
        .query_row(
            "SELECT cut_over_date FROM opening_balances ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(sql)?
        .flatten();
    let stale = stale_sources(tx, &plan)?;
    let state = if total == 0 { "zero" } else { "available" };
    let source_hash = hash(&json!({"planHash":plan["planHash"],"periodClose":period_close}));
    checked_plan(
        json!({"ok":true,"workbench":{"scope":{"from":input.from,"to":input.to,"cutOverDate":cutover},"state":state,"completeness":{"state":state,"reasonCode":if total==0{"no_unmatched_bank_rows"}else{"complete_native_supported_purchase_slice"},"nextAction":"Review visible blockers; supported native DKK purchase proof only"},"rows":page,"page":{"cursor":cursor,"limit":limit,"total":selection_total,"nextCursor":next},"counts":counts,"population":{"total":total,"ready":ready,"blockers":total-ready},"selection":{"total":selection_total,"ready":selection_ready,"blockers":selection_total-selection_ready},"staleSources":stale,"sourceHash":source_hash,"periodClose":period_close,"plan":{"planHash":plan["planHash"],"candidateSetHash":plan["candidateSetHash"],"readyCount":plan["items"].as_array().unwrap().iter().filter(|i|i["partition"]=="ready").count()}}}),
    )
}
fn stale_sources(tx: &Transaction<'_>, current: &Value) -> Result<Vec<Value>, Error> {
    let prior:Option<String>=tx.query_row("SELECT r.plan_json FROM bookkeeping_batch_revisions r JOIN bookkeeping_batch_runs b ON b.id=r.run_id WHERE b.accounting_from=?1 AND b.accounting_to=?2 AND b.bank_from=?3 AND b.bank_to=?4 ORDER BY r.id DESC LIMIT 1",params![text(current,"accountingFrom")?,text(current,"accountingTo")?,text(current,"bankFrom")?,text(current,"bankTo")?],|r|r.get(0)).optional().map_err(sql)?;
    let Some(prior) = prior else {
        return Ok(vec![]);
    };
    let prior: Value = serde_json::from_str(&prior).map_err(|_| fail("invalid historical plan"))?;
    let Some(old) = prior["items"].as_array() else {
        return Ok(vec![]);
    };
    let new = current["items"].as_array().unwrap();
    let mut out = vec![];
    for i in old {
        let matched = new.iter().find(|n| n["actionKey"] == i["actionKey"]);
        let changed = match matched {
            None => Some("removed_from_candidate_set"),
            Some(n) if n["evidenceHash"] != i["evidenceHash"] || n["detail"] != i["detail"] => {
                Some("evidence_changed")
            }
            _ => None,
        };
        if let Some(reason) = changed {
            out.push(json!({"bankTransactionId":i["bankTransactionId"],"actionKey":i["actionKey"],"changedSource":reason}))
        }
    }
    for i in new {
        if !old.iter().any(|o| o["actionKey"] == i["actionKey"]) {
            out.push(json!({"bankTransactionId":i["bankTransactionId"],"actionKey":i["actionKey"],"changedSource":"added_to_candidate_set"}))
        }
    }
    Ok(out)
}
