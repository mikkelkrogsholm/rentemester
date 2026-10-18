use super::*;
use rentemester_runtime::auth::{AuthError, CookieProfile, CookieVerifier};
use serde_json::json;
struct FixedClock(i64);
impl Clock for FixedClock {
    fn now_unix_ms(&self) -> Result<i64, AuthError> {
        Ok(self.0)
    }
}
#[test]
#[ignore = "canonical synthetic real journal reversal process helper"]
fn child() {
    let config: Value = serde_json::from_str(&std::env::var("RM_POST_REQUEST").unwrap()).unwrap();
    let mut c = Connection::open(std::env::var("RM_POST_CONTROL").unwrap()).unwrap();
    let mut l = Connection::open(std::env::var("RM_POST_COMPANY").unwrap()).unwrap();
    for db in [&c, &l] {
        db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL")
            .unwrap();
        db.busy_timeout(std::time::Duration::from_millis(
            config["busyMs"].as_u64().unwrap_or(100),
        ))
        .unwrap();
    }
    let verifier = CookieVerifier::new(
        "w",
        CookieProfile::Secure,
        config["signer"].as_str().unwrap(),
    )
    .unwrap();
    let proof = verifier.verify(config["cookie"].as_str(), config["service"].as_str());
    let output = match proof {
        Err(e) => json!({"outcome":"not_committed","code":e.code()}),
        Ok(proof) => {
            let input: Input = serde_json::from_value(config["input"].clone()).unwrap();
            let clock = FixedClock(config["now"].as_i64().unwrap());
            match execute(
                &mut c,
                &mut l,
                &proof,
                &clock,
                CompanySelection {
                    workspace: config["workspace"].as_str().unwrap_or("w"),
                    slug: config["slug"].as_str().unwrap_or("a"),
                    registered: config["registered"].as_bool().unwrap_or(true),
                    archived: config["archived"].as_bool().unwrap_or(false),
                },
                &Policy {
                    max_future_days: config["maxFutureDays"].as_u64().unwrap_or(45) as u32,
                },
                input,
            ) {
                Ok(done) => {
                    json!({"outcome":"committed","result":done.value.result,"receipt":done.value.receipt,"controlReusable":done.control_reusable})
                }
                Err(f) => {
                    json!({"outcome":if f.fence.stage==FailureStage::NotCommitted{"not_committed"}else{"unknown"},"code":f.fence.error.code,"diagnostic":f.fence.error.message,"business":f.business,"controlReusable":f.fence.control_reusable,"companyReusable":f.fence.company_reusable})
                }
            }
        }
    };
    std::fs::write(
        std::env::var("RM_POST_OUTPUT").unwrap(),
        serde_json::to_vec(&output).unwrap(),
    )
    .unwrap();
}
