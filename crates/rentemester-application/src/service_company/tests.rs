use super::*;
use rentemester_runtime::auth::AuthError;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
struct FixedClock {
    now: i64,
    step: i64,
    calls: AtomicUsize,
}
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.now
            + if self.calls.fetch_add(1, Ordering::SeqCst) > 0 {
                self.step
            } else {
                0
            })
    }
}
#[test]
fn failure_debug_never_discloses_error_detail_or_business_payload() {
    let mut f = initial(Error::new("synthetic", "SYNTHETIC_SECRET_DETAIL"));
    f.rejection = Some(json!({"secret":"SYNTHETIC_SECRET_PAYLOAD"}));
    let shown = format!("{f:?}");
    assert!(!shown.contains("SYNTHETIC_SECRET"));
    assert!(shown.contains("NotAttempted"));
}
#[test]
#[ignore = "canonical synthetic service company quota/post/reversal process helper"]
fn child() {
    let v: Value = serde_json::from_str(&std::env::var("RM_SERVICE_REQUEST").unwrap()).unwrap();
    let mut control = Connection::open(std::env::var("RM_SERVICE_CONTROL").unwrap()).unwrap();
    let mut company = Connection::open(std::env::var("RM_SERVICE_COMPANY").unwrap()).unwrap();
    for db in [&control, &company] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        db.busy_timeout(std::time::Duration::from_millis(
            v["busyMs"].as_u64().unwrap_or(100),
        ))
        .unwrap();
    }
    if v["controlQueryOnly"] == true {
        control.execute_batch("PRAGMA query_only=ON").unwrap();
    }
    if v["companyQueryOnly"] == true {
        company.execute_batch("PRAGMA query_only=ON").unwrap();
    }
    if v["controlTransaction"] == true {
        control.execute_batch("BEGIN").unwrap();
    }
    if v["companyTransaction"] == true {
        company.execute_batch("BEGIN").unwrap();
    }
    let key = ServiceKey::new(
        v["credentialWorkspace"].as_str().unwrap_or("w"),
        v["secret"].as_str().unwrap(),
    )
    .unwrap();
    let clock = FixedClock {
        now: v["now"].as_i64().unwrap(),
        step: v["clockStep"].as_i64().unwrap_or(0),
        calls: AtomicUsize::new(0),
    };
    let context = Context {
        key: &key,
        clock: &clock,
        selection: CompanySelection {
            workspace: v["workspace"].as_str().unwrap_or("w"),
            slug: v["slug"].as_str().unwrap_or("a"),
            registered: v["registered"].as_bool().unwrap_or(true),
            archived: v["archived"].as_bool().unwrap_or(false),
        },
        max_future_days: 45,
    };
    let result = if v["operation"] == "reverse" {
        execute_reversal(
            &mut control,
            &mut company,
            context,
            serde_json::from_value(v["input"].clone()).unwrap(),
        )
    } else {
        execute_post(
            &mut control,
            &mut company,
            context,
            serde_json::from_value(v["input"].clone()).unwrap(),
        )
    };
    let output = match result {
        Ok(r) => {
            json!({"ok":true,"usage":r.usage,"business":r.business,"result":r.result,"receipt":r.receipt,"controlReusable":r.control_reusable})
        }
        Err(f) => {
            json!({"ok":false,"usage":f.usage,"business":f.business,"code":f.error.code,"rejection":f.rejection,"controlReusable":f.control_reusable,"companyReusable":f.company_reusable})
        }
    };
    std::fs::write(
        std::env::var("RM_SERVICE_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}
