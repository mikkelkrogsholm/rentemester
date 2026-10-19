//! Ephemeral explicit active-app authority and exact normal-due evidence. No scheduler dates or money math.
use crate::scheduled_recurring_state::{Action, Review, Snapshot};
use serde_json::{Value, json};
#[derive(Clone, PartialEq)]
pub struct Binding {
    pub origin: String,
    pub local: Option<String>,
    pub company: String,
    pub principal: String,
    pub ledger: String,
    pub template_id: i64,
    pub template_hash: Value,
    pub template: Value,
}
impl Binding {
    pub fn reviewed(origin: &str, local: Option<&str>, data: &Snapshot) -> Result<Self, String> {
        let s = data
            .selected
            .as_ref()
            .ok_or("Vælg en kanonisk skabelon før aktiv-app-start")?;
        let _ = data.reviewed_template()?;
        let p = data
            .plan
            .as_ref()
            .ok_or("Aktiv-app-start kræver fuld readonly serverplan")?;
        if !data.can_write()
            || !data.same_owner()
            || s["state"] != "enabled"
            || !p["plan"]["template"].is_object()
            || p["plan"]["template"]["id"] != s["templateId"]
        {
            return Err("Aktiv-app-start kræver samme samtykkende bruger, aktuel adgang, enabled-kø og fuld server-skabelonkilde".into());
        }
        let template_id = s["templateId"]
            .as_i64()
            .filter(|id| *id > 0)
            .ok_or("Skabelon-ID mangler")?;
        Ok(Self {
            origin: origin.into(),
            local: local.map(str::to_owned),
            company: data.company.clone(),
            principal: data.principal.clone(),
            ledger: data.ledger.clone(),
            template_id,
            template_hash: s["templateHash"].clone(),
            template: p["plan"]["template"].clone(),
        })
    }
}
pub enum Decision {
    Idle,
    Dispatch(Review),
}
pub fn next(binding: &Binding, data: &Snapshot, pending: bool) -> Result<Decision, String> {
    if pending {
        return Err(
            "En tidligere handling er uafklaret. Automatik stoppet; læs og afklar manuelt.".into(),
        );
    }
    let s = data
        .selected
        .as_ref()
        .ok_or("Skabelon/køkilde er ikke længere synlig")?;
    let p = data
        .plan
        .as_ref()
        .ok_or("Readonly plan er utilgængelig; manuel kontrol kræves")?;
    let p = &p["plan"];
    if data.company != binding.company
        || data.principal != binding.principal
        || data.ledger != binding.ledger
        || !data.can_write()
        || !data.same_owner()
        || s["state"] != "enabled"
        || s["templateId"] != binding.template_id
        || s["templateHash"] != binding.template_hash
        || p["template"] != binding.template
        || p["templateHash"] != binding.template_hash
        || p["stateHash"] != s["stateHash"]
        || p["asOfDate"] != data.status["scheduledRecurring"]["asOfDate"]
    {
        return Err("Aktiv bruger, adgang, ledger, samtykke, skabelon eller eksakt kildeplan ændrede sig. Manuel kontrol/start kræves.".into());
    }
    if !s["error"].is_null()
        || (!s["lastRun"].is_null() && s["lastRun"]["status"] != "succeeded")
        || s["missedCount"].as_u64() != Some(0)
        || p["truncated"] != false
        || p["missedCount"].as_u64() != Some(0)
        || !p["missed"].as_array().is_some_and(Vec::is_empty)
        || !p["queued"].as_array().is_some_and(Vec::is_empty)
        || !s["queued"].as_array().is_some_and(Vec::is_empty)
    {
        return Err("Fejl/uafklaret, missed, køsatte eller trunkerede forekomster kræver eksplicit manuel planreview; ingen automatisk catch-up/retry.".into());
    }
    if p["normalDue"].is_null() {
        return Ok(Decision::Idle);
    }
    if p["normalDue"]["issueDate"] != p["asOfDate"] {
        return Err("Normal forekomst er ikke serverens faktiske dag. Ingen dato eller catch-up udledes lokalt.".into());
    }
    let index = p["normalDue"]["periodIndex"]
        .as_u64()
        .ok_or("Normal forekomst mangler eksakt periodIndex")?;
    Ok(Decision::Dispatch(Review::prepare(
        data,
        Action::Dispatch,
        "",
        &[],
        Some(index),
        false,
    )?))
}
/// A stop linearizes against admission, which persists the marker before admitting the one request.
/// Once admitted, an existing request may finish; stopping never creates a retry signal.
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU8, Ordering},
};
pub struct Gate {
    state: AtomicU8,
}
impl Gate {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(1),
        }
    }
    pub fn active(&self) -> bool {
        self.state.load(Ordering::SeqCst) & 1 != 0
    }
    pub fn stop(&self) {
        self.state.fetch_and(!1, Ordering::SeqCst);
    }
    pub fn admit(&self, persist: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        if self.state.load(Ordering::SeqCst) != 1 {
            return Err("Aktiv-app-autoritet er stoppet eller allerede i brug".into());
        }
        persist()?;
        self.state.compare_exchange(1,3,Ordering::SeqCst,Ordering::SeqCst).map(|_|()).map_err(|_|"Stoppet under markerens lagring; intet sendt. Markøren bevares til manuel afklaring.".into())
    }
    pub fn finish(&self) {
        self.state.fetch_and(!2, Ordering::SeqCst);
    }
}
pub struct Lane(pub Arc<AtomicBool>);
impl Lane {
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }
    pub fn busy(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    pub fn reserve(&self) -> Option<LanePermit> {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| LanePermit(self.0.clone()))
    }
}
pub struct LanePermit(Arc<AtomicBool>);
impl Drop for LanePermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}
pub fn receipt(review: &Review, v: &Value) -> Result<Option<i64>, String> {
    if !review.receipt(v) {
        return Err("Dispatchkvitteringens template/period/document/hash eller status kan ikke bekræftes. Udfaldsmarkør bevares.".into());
    }
    if v["dispatch"]["status"] == "failed" {
        return Ok(None);
    }
    if v["generation"]["templateId"] != review.display["templateId"]
        || v["generation"]["periodIndex"] != review.display["periodIndex"]
    {
        return Err(
            "Dispatchkvittering tilhører en anden template/period. Udfaldsmarkør bevares.".into(),
        );
    }
    v["generation"]["documentId"]
        .as_i64()
        .filter(|id| *id > 0)
        .map(Some)
        .ok_or("Præcis original documentId mangler".into())
}
pub fn verified_original(
    review: &Review,
    receipt: &Value,
    data: &Snapshot,
    pdf: &Value,
) -> Result<Value, String> {
    let id = crate::scheduled_dispatcher_state::receipt(review, receipt)?
        .ok_or("Kendt failed dispatch har ingen udstedt original")?;
    if !review.readback(receipt, data) {
        return Err(
            "Kanonisk køtilstand matcher ikke den accepterede kvittering; manuel kontrol kræves"
                .into(),
        );
    }
    let no = receipt["generation"]["invoiceNumber"]
        .as_str()
        .ok_or("Fakturanummer mangler")?;
    let invoice = data.invoices["invoices"]["invoices"]
        .as_array()
        .and_then(|a| {
            a.iter().find(|r| {
                r["documentId"] == id
                    && r["invoiceNo"] == no
                    && r["posted"] == false
                    && r["lastEmailedAt"].is_null()
            })
        })
        .ok_or("Præcis udstedt/ubogført/ikke-afsendt faktura er ikke kanonisk genlæst")?;
    let bytes = pdf["pdf"]["bytes"]
        .as_array()
        .filter(|a| a.len() >= 5 && a.len() <= 16 * 1024 * 1024)
        .ok_or("Original PDF mangler eller overskrider grænsen")?;
    if pdf["ok"] != true
        || pdf["pdf"]["contentType"] != "application/pdf"
        || !bytes.iter().all(|v| v.as_u64().is_some_and(|n| n <= 255))
        || !bytes
            .iter()
            .take(5)
            .zip(b"%PDF-")
            .all(|(v, b)| v.as_u64() == Some(u64::from(*b)))
    {
        return Err("Originalens PDF-signatur/MIME/bytes kan ikke bekræftes".into());
    }
    Ok(
        json!({"templateId":review.display["templateId"],"periodIndex":review.display["periodIndex"],"documentId":id,"invoiceNumber":no,"posted":invoice["posted"],"sent":false,"pdfBytes":bytes.len()}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn data() -> Snapshot {
        let template = json!({"id":7,"name":"Synthetic fixed source","interval":"monthly","intervalCount":1,"firstIssueDate":"2026-10-19","paymentTermsDays":14,"deliveryPeriodMode":"issue_month","payload":{"seller":{"name":"Synthetic Seller"},"buyer":{"name":"Synthetic Buyer"},"currency":"DKK","lines":[{"description":"Synthetic service","quantity":1,"unitPriceExVat":12.34}]}});
        let row = json!({"templateId":7,"templateHash":"a".repeat(64),"state":"enabled","stateHash":"b".repeat(64),"ownerUserId":"owner","lastRun":null,"nextOccurrence":{"periodIndex":5,"issueDate":"2026-10-19"},"missedCount":0,"queued":[],"error":null,"history":[]});
        Snapshot::parse("synthetic",Some(7),json!({"ok":true,"recurringInvoices":{"slug":"synthetic","templates":[{"id":7,"active":true,"name":"Synthetic fixed source"}]}}),json!({"ok":true,"invoices":{"invoices":[{"documentId":11,"invoiceNo":"SYN-11","posted":false,"lastEmailedAt":null}]}}),json!({"ok":true,"scheduledRecurring":{"asOfDate":"2026-10-19","timeZone":"Europe/Copenhagen","localService":false,"rows":[row]}}),Some(json!({"ok":true,"plan":{"templateId":7,"templateHash":"a".repeat(64),"stateHash":"b".repeat(64),"asOfDate":"2026-10-19","normalDue":{"periodIndex":5,"issueDate":"2026-10-19"},"missed":[],"missedCount":0,"truncated":false,"queued":[],"planHash":"c".repeat(64),"template":template}})),None,json!({"ok":true,"user":{"id":"owner"},"companies":[{"slug":"synthetic","role":"owner"}]}),json!({"ok":true,"journal":{"slug":"synthetic","ledgerUuid":"immutable-ledger"}})).unwrap()
    }
    fn binding(d: &Snapshot) -> Binding {
        Binding::reviewed(
            "https://synthetic.invalid",
            Some("trusted-local-identity"),
            d,
        )
        .unwrap()
    }
    fn review(d: &Snapshot) -> Review {
        match next(&binding(d), d, false).unwrap() {
            Decision::Dispatch(r) => r,
            _ => panic!("expected actual normalDue"),
        }
    }
    fn receipt_value(d: &Snapshot) -> Value {
        json!({"ok":true,"scheduledRecurring":d.selected,"dispatch":{"status":"succeeded"},"generation":{"templateId":7,"periodIndex":5,"documentId":11,"invoiceNumber":"SYN-11"}})
    }
    fn pdf() -> Value {
        json!({"ok":true,"pdf":{"contentType":"application/pdf","bytes":[37,80,68,70,45,49]}})
    }
    #[test]
    fn explicit_activation_requires_current_enabled_consenting_owner_and_full_immutable_source() {
        let d = data();
        let b = binding(&d);
        assert_eq!(b.template, d.plan.as_ref().unwrap()["plan"]["template"]);
        for change in 0..6 {
            let mut bad = d.clone();
            match change {
                0 => bad.principal = "other".into(),
                1 => bad.role = "reader".into(),
                2 => bad.selected.as_mut().unwrap()["state"] = json!("paused"),
                3 => bad.plan = None,
                4 => bad.plan.as_mut().unwrap()["plan"]["template"] = Value::Null,
                _ => bad.plan.as_mut().unwrap()["plan"]["template"]["id"] = json!(8),
            }
            assert!(Binding::reviewed("https://synthetic.invalid", None, &bad).is_err());
        }
    }
    #[test]
    fn empty_normal_due_is_readonly_idle_and_has_no_synthetic_occurrence_or_date() {
        let mut d = data();
        let b = binding(&d);
        d.plan.as_mut().unwrap()["plan"]["normalDue"] = Value::Null;
        assert!(matches!(next(&b, &d, false).unwrap(), Decision::Idle));
        let mut future = data();
        let b = binding(&future);
        future.plan.as_mut().unwrap()["plan"]["normalDue"]["issueDate"] = json!("2026-10-20");
        assert!(next(&b, &future, false).is_err());
        future.plan.as_mut().unwrap()["plan"]["normalDue"]["issueDate"] = json!("2026-10-18");
        assert!(next(&b, &future, false).is_err());
    }
    #[test]
    fn current_scope_ledger_consent_role_hash_and_immutable_payload_changes_stop() {
        let d = data();
        let b = binding(&d);
        for change in 0..10 {
            let mut changed = d.clone();
            match change {
                0 => changed.principal = "other".into(),
                1 => changed.company = "other".into(),
                2 => changed.ledger = "other-ledger".into(),
                3 => changed.role = "reader".into(),
                4 => changed.selected.as_mut().unwrap()["ownerUserId"] = json!("other"),
                5 => changed.selected.as_mut().unwrap()["state"] = json!("cancelled"),
                6 => changed.selected.as_mut().unwrap()["templateHash"] = json!("d".repeat(64)),
                7 => {
                    changed.plan.as_mut().unwrap()["plan"]["template"]["payload"]["buyer"] =
                        json!({"name":"Changed source"})
                }
                8 => changed.plan.as_mut().unwrap()["plan"]["stateHash"] = json!("d".repeat(64)),
                _ => changed.plan = None,
            }
            assert!(next(&b, &changed, false).is_err());
        }
    }
    #[test]
    fn missed_queued_truncated_error_unresolved_and_private_pending_never_auto_catch_up() {
        let d = data();
        let b = binding(&d);
        assert!(next(&b, &d, true).is_err());
        for change in 0..11 {
            let mut changed = d.clone();
            match change {
                0 => changed.selected.as_mut().unwrap()["error"] = json!("source_refused"),
                1 => changed.selected.as_mut().unwrap()["lastRun"] = json!({"status":"failed"}),
                2 => changed.selected.as_mut().unwrap()["lastRun"] = json!({"status":"unresolved"}),
                3 => changed.plan.as_mut().unwrap()["plan"]["missedCount"] = json!(1),
                4 => {
                    changed.plan.as_mut().unwrap()["plan"]["missed"] =
                        json!([{"periodIndex":1,"issueDate":"2026-09-19"}])
                }
                5 => changed.plan.as_mut().unwrap()["plan"]["truncated"] = json!(true),
                6 => {
                    changed.plan.as_mut().unwrap()["plan"]["queued"] =
                        json!([{"periodIndex":1,"issueDate":"2026-09-19"}])
                }
                7 => {
                    changed.selected.as_mut().unwrap()["queued"] =
                        json!([{"periodIndex":1,"issueDate":"2026-09-19"}])
                }
                8 => {
                    changed.selected.as_mut().unwrap()["lastRun"] =
                        json!({"status":"future-unknown"})
                }
                9 => changed.selected.as_mut().unwrap()["lastRun"] = json!({}),
                _ => changed.selected.as_mut().unwrap()["missedCount"] = json!(1),
            }
            assert!(next(&b, &changed, false).is_err());
        }
    }
    #[test]
    fn exact_normal_due_body_is_frozen_with_no_money_date_key_or_actor_injection() {
        let d = data();
        let r = review(&d);
        assert_eq!(
            r.display,
            json!({"templateId":7,"planHash":"c".repeat(64),"periodIndex":5,"confirm":true})
        );
        assert_eq!(serde_json::from_slice::<Value>(&r.body).unwrap(), r.display);
        for k in [
            "idempotencyKey",
            "actor",
            "asOfDate",
            "amount",
            "invoice",
            "seller",
        ] {
            assert!(r.display.get(k).is_none());
        }
        let marker = r.marker("https://synthetic.invalid", None);
        for k in [
            "body",
            "plan",
            "generation",
            "periodIndex",
            "amount",
            "cookie",
        ] {
            assert!(marker.get(k).is_none());
        }
    }
    #[test]
    fn persist_before_post_stop_after_persist_and_shared_manual_auto_lane_are_fail_closed() {
        let gate = Gate::new();
        assert!(gate.admit(|| Err("disk refused".into())).is_err());
        let mut persisted = false;
        gate.admit(|| {
            persisted = true;
            Ok(())
        })
        .unwrap();
        assert!(persisted);
        assert!(
            gate.admit(|| panic!("second admission must not persist"))
                .is_err()
        );
        gate.stop();
        assert!(!gate.active());
        gate.finish();
        assert!(
            gate.admit(|| panic!("stopped admission must not persist"))
                .is_err()
        );
        let stopped = Gate::new();
        let mut marker = false;
        assert!(
            stopped
                .admit(|| {
                    marker = true;
                    stopped.stop();
                    Ok(())
                })
                .is_err()
        );
        assert!(
            marker,
            "marker remains after stop during fsync; no POST admitted"
        );
        let lane = Lane::new();
        let manual = lane.reserve().unwrap();
        assert!(lane.reserve().is_none());
        drop(manual);
        let automatic = lane.reserve().unwrap();
        assert!(lane.busy());
        drop(automatic);
        assert!(!lane.busy());
        for status in [403, 422] {
            assert!(crate::scheduled_recurring_state::definite_refusal(status));
        }
        for status in [408, 500, 503] {
            assert!(!crate::scheduled_recurring_state::definite_refusal(status));
        }
    }
    #[test]
    fn malformed_wrong_template_period_receipt_and_known_terminal_failed_never_become_success() {
        let d = data();
        let r = review(&d);
        let v = receipt_value(&d);
        assert_eq!(receipt(&r, &v).unwrap(), Some(11));
        for key in ["templateId", "periodIndex", "documentId"] {
            let mut wrong = v.clone();
            wrong["generation"][key] = json!(99);
            if key == "documentId" {
                wrong["generation"][key] = json!(0);
            }
            assert!(receipt(&r, &wrong).is_err());
        }
        let mut missing = v.clone();
        missing["dispatch"] = Value::Null;
        assert!(receipt(&r, &missing).is_err());
        let mut failed = v;
        failed["dispatch"] = json!({"status":"failed","error":"source_refused"});
        failed["generation"] = Value::Null;
        assert_eq!(receipt(&r, &failed).unwrap(), None);
    }
    #[test]
    fn success_requires_exact_canonical_invoice_unposted_unsent_and_physical_pdf_readback() {
        let d = data();
        let r = review(&d);
        let v = receipt_value(&d);
        let proof = verified_original(&r, &v, &d, &pdf()).unwrap();
        assert_eq!(proof["documentId"], 11);
        assert_eq!(proof["posted"], false);
        assert_eq!(proof["sent"], false);
        for change in 0..4 {
            let mut bad = d.clone();
            match change {
                0 => bad.invoices["invoices"]["invoices"][0]["documentId"] = json!(12),
                1 => bad.invoices["invoices"]["invoices"][0]["posted"] = json!(true),
                2 => bad.invoices["invoices"]["invoices"][0]["lastEmailedAt"] = json!("2026-10-19"),
                _ => bad.selected.as_mut().unwrap()["stateHash"] = json!("f".repeat(64)),
            }
            assert!(verified_original(&r, &v, &bad, &pdf()).is_err());
        }
        for change in 0..3 {
            let mut bad = pdf();
            match change {
                0 => bad["pdf"]["contentType"] = json!("text/html"),
                1 => bad["pdf"]["bytes"][0] = json!(0),
                _ => bad["pdf"]["bytes"][5] = json!(256),
            }
            assert!(verified_original(&r, &v, &d, &bad).is_err());
        }
    }
}
