use super::*;
use serde_json::{Value, json};
#[test]
fn invalid_input_never_calls_continuation() {
    let file = tempfile::NamedTempFile::new().unwrap();
    let mut db = Connection::open(file.path()).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON;PRAGMA synchronous=FULL;")
        .unwrap();
    let result = execute(
        &mut db,
        "invalid",
        "synthetic",
        |_, _| -> Result<(), Error> { panic!("invalid input reached continuation") },
    );
    assert_eq!(result.unwrap_err().error.code, "invalid_credentials");
    assert!(db.is_autocommit());
}
#[test]
#[ignore = "synthetic canonical control native helper selected by independent process harness"]
fn probe() {
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let request: Value = serde_json::from_str(&input).unwrap();
    if request["mode"] == "email" {
        let output: Vec<_> = request["emails"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| password_credential::lookup_email(v.as_str().unwrap()).ok())
            .collect();
        println!("RM_LOGIN_RESULT={}", json!({"emails":output}));
        return;
    }
    let mut db = Connection::open(request["control"].as_str().unwrap()).unwrap();
    db.busy_timeout(std::time::Duration::ZERO).unwrap();
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")
        .unwrap();
    if let Some(sql) = request["preflight"].as_str() {
        db.execute_batch(sql).unwrap();
    }
    let result = execute(
        &mut db,
        request["email"].as_str().unwrap(),
        request["password"].as_str().unwrap(),
        |tx, factor| {
            tx.execute("INSERT INTO main.workspace_audit(event_type,entity_type,actor) VALUES('synthetic-password-continuation','test','agent:synthetic')", []).map_err(|_| Error::new("private-canary", "secret callback diagnostic"))?;
            if request["fail"] == true {
                return Err(Error::new("private-canary", "secret callback diagnostic"));
            }
            Ok(match factor.next {
                password_credential::NextFactor::Challenge => "challenge",
                password_credential::NextFactor::Enrollment => "enrollment",
            })
        },
    );
    let output = match result {
        Ok(next) => json!({"ok":true,"next":next}),
        Err(e) => {
            json!({"ok":false,"code":e.error.code,"message":e.error.message,"stage":format!("{:?}",e.stage),"reusable":e.connection_reusable})
        }
    };
    println!("RM_LOGIN_RESULT={output}");
}
