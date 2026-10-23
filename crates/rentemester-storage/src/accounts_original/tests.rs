use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn company() -> Connection {
    let (mut control, mut db) = (
        Connection::open_in_memory().unwrap(),
        Connection::open_in_memory().unwrap(),
    );
    crate::fresh::initialize_empty_pair(&mut control, &mut db, "Synthetic original chart").unwrap();
    db
}
fn add(tx: &Transaction<'_>, no: &str, kind: &str) -> Value {
    execute(
        tx,
        Operation::Add,
        &json!({"input":{"accountNo":no,"name":" Synthetic account ","type":kind},"confirm":true}),
        "service_account:synthetic",
    )
    .unwrap()
}
fn count(tx: &Transaction<'_>, table: &str) -> i64 {
    tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}
#[test]
fn exact_list_defaults_direct_insert_and_refusals() {
    let mut db = company();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    for (no, kind, normal) in [
        (" 1 ", "asset", "debit"),
        ("2", "expense", "debit"),
        ("3", "income", "credit"),
        ("4", "liability", "credit"),
        ("5", "equity", "credit"),
        ("6", "vat", "credit"),
    ] {
        let result = add(&tx, no, kind);
        assert_eq!(
            result,
            json!({"ok":true,"accountNo":no.trim(),"appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"],"errors":[]})
        );
        let stored:(String,String,Option<String>,bool) = tx.query_row("SELECT name,normal_balance,default_vat_code,allow_direct_posting FROM accounts WHERE account_no=?1",[no.trim()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(
            stored,
            ("Synthetic account".into(), normal.into(), None, true)
        );
    }
    let body = json!({"input":{"accountNo":"0","name":"Synthetic control","type":" expense ","normalBalance":" ","defaultVatCode":" ","allowDirectPosting":false},"confirm":true});
    // Trigger proves false is present on the INSERT itself, not an after-the-fact update.
    tx.execute_batch("CREATE TEMP TRIGGER source_flag_insert BEFORE INSERT ON accounts WHEN NEW.account_no='0' AND NEW.allow_direct_posting<>0 BEGIN SELECT RAISE(ABORT,'flag must be atomic'); END;").unwrap();
    assert_eq!(
        execute(&tx, Operation::Add, &body, "user:synthetic").unwrap()["ok"],
        true
    );
    tx.execute("UPDATE accounts SET active=0 WHERE account_no='0'", [])
        .unwrap();
    let listed = execute(&tx, Operation::List, &json!({}), "").unwrap();
    assert_eq!(listed["count"], 7);
    assert_eq!(
        listed["rows"][0],
        json!({"account_no":"0","name":"Synthetic control","type":"expense","default_vat_code":null})
    );
    let before = count(&tx, "audit_log");
    for bad in [
        body.clone(),
        json!({"input":{"accountNo":"new","name":"Synthetic","type":"invalid"},"confirm":true}),
        json!({"input":{"accountNo":"new","name":"Synthetic","type":"asset","normalBalance":"wrong"},"confirm":true}),
        json!({"input":{"accountNo":"new","name":"Synthetic","type":"asset","defaultVatCode":"unknown"},"confirm":true}),
        json!({"input":{"accountNo":"new","name":"Synthetic","type":"asset","unexpected":true},"confirm":true}),
        json!({"input":{"accountNo":"new","name":"Synthetic","type":"asset"},"confirm":false}),
    ] {
        let result = execute(&tx, Operation::Add, &bad, "user:synthetic").unwrap();
        assert_eq!(result["ok"], false);
        assert!(!result["errors"][0].as_str().unwrap().is_empty());
        assert_eq!(count(&tx, "accounts"), 7);
        assert_eq!(count(&tx, "audit_log"), before);
    }
    let actors: i64=tx.query_row("SELECT COUNT(*) FROM audit_log WHERE event_type='account_created' AND actor='service_account:synthetic'",[],|r|r.get(0)).unwrap();
    assert_eq!(actors, 6);
    assert!(execute(&tx, Operation::List, &json!({"company":"other"}), "").is_err());
}
#[test]
fn eight_roles_source_reconfirmation_and_native_idempotency_fx_unchanged() {
    let mut db = company();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    let empty = execute(&tx, Operation::RolesStatus, &json!({}), "").unwrap();
    assert_eq!(empty["missing"], json!(setup::ROLES.map(|r| r.0)));
    assert_eq!(empty["roles"].as_array().unwrap().len(), 8);
    for (i, (role, kind, normal)) in setup::ROLES.iter().enumerate() {
        let no = format!("{i}");
        let body = json!({"input":{"accountNo":no,"name":"Synthetic role","type":kind,"normalBalance":normal},"confirm":true});
        assert_eq!(
            execute(&tx, Operation::Add, &body, "user:synthetic").unwrap()["ok"],
            true
        );
        let result = execute(
            &tx,
            Operation::RoleConfirm,
            &json!({"role":role,"accountNo":no,"confirm":true}),
            "service_account:synthetic",
        )
        .unwrap();
        assert_eq!(
            result["resolution"]["confirmedBy"],
            "service_account:synthetic"
        );
    }
    let body = json!({"role":"bank","accountNo":"0","confirm":true});
    let result = execute(&tx, Operation::RoleConfirm, &body, "user:second").unwrap();
    assert_eq!(result["resolution"]["version"], 2);
    assert_eq!(result["status"], "complete");
    assert_eq!(result["missing"], json!([]));
    assert_eq!(result["ambiguous"], json!([]));
    assert!(result.get("errors").is_none());
    let native: setup::Role = serde_json::from_value(body).unwrap();
    let before = count(&tx, "audit_log");
    assert_eq!(
        setup::confirm_role(&tx, &native, "native").unwrap(),
        result["resolution"]
    );
    assert_eq!(count(&tx, "audit_log"), before);
    assert_eq!(count(&tx, "account_role_mappings"), 9);
    for role in ["fx_gain", "unsupported", "output_vat"] {
        let result = execute(
            &tx,
            Operation::RoleConfirm,
            &json!({"role":role,"accountNo":"0","confirm":true}),
            "user:synthetic",
        )
        .unwrap();
        assert_eq!(result["ok"], false);
        assert_eq!(result["status"], "complete");
        assert_eq!(count(&tx, "audit_log"), before);
        assert_eq!(count(&tx, "account_role_mappings"), 9);
    }
    add(&tx, "FX", "income");
    let fx = setup::Role {
        role: "fx_gain".into(),
        account_no: "FX".into(),
        confirm: true,
    };
    assert_eq!(setup::confirm_role(&tx, &fx, "native").unwrap()["ok"], true);
    assert_eq!(
        setup::accounts(&tx, "synthetic").unwrap()["accountRoles"]["resolutions"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert_eq!(
        execute(&tx, Operation::RolesStatus, &json!({}), "").unwrap()["roles"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
}
#[test]
fn proposals_are_deterministic_diagnostics_not_authority() {
    let mut db = company();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    add(&tx, "B", "asset");
    add(&tx, "A", "asset");
    add(&tx, "X", "expense");
    tx.execute_batch("INSERT INTO account_role_proposals(role,account_no,source) VALUES('bank','B','z'),('bank','A','z'),('bank','A','a'),('bank','X','a');").unwrap();
    let before = tx.total_changes();
    let status = execute(&tx, Operation::RolesStatus, &json!({}), "").unwrap();
    assert_eq!(status["status"], "ambiguous");
    assert_eq!(status["ambiguous"], json!(["bank"]));
    assert!(
        !status["missing"]
            .as_array()
            .unwrap()
            .contains(&json!("bank"))
    );
    assert_eq!(status["proposals"], status["candidates"]);
    assert_eq!(status["proposals"].as_array().unwrap().len(), 4);
    assert_eq!(status["proposals"][0]["accountNo"], "A");
    assert_eq!(status["proposals"][0]["source"], "a");
    assert!(status["proposals"][0].get("reason").is_none());
    assert_eq!(status["proposals"][3]["compatible"], false);
    assert!(status["proposals"][3]["reason"].is_string());
    assert!(
        status["reasons"][0]["reason"]
            .as_str()
            .unwrap()
            .ends_with("; 2 unconfirmed proposals")
    );
    assert_eq!(status["roles"][0]["ok"], false);
    assert_eq!(tx.total_changes(), before);
    let confirmed = execute(
        &tx,
        Operation::RoleConfirm,
        &json!({"role":"bank","accountNo":"A","confirm":true}),
        "user:synthetic",
    )
    .unwrap();
    assert_eq!(confirmed["ambiguous"], json!([]));
    assert_eq!(confirmed["proposals"], json!([]));
    let states: Vec<String>=tx.prepare("SELECT status FROM account_role_proposals WHERE role='bank' ORDER BY account_no,source").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<Result<_,_>>().unwrap();
    assert_eq!(states, vec!["accepted", "accepted", "rejected", "rejected"]);
}
#[test]
fn failed_audit_rolls_back_account_and_reconfirmation_even_if_outer_commits() {
    let mut db = company();
    let tx = db
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    add(&tx, "A", "asset");
    let role = json!({"role":"bank","accountNo":"A","confirm":true});
    assert_eq!(
        execute(&tx, Operation::RoleConfirm, &role, "user:synthetic").unwrap()["ok"],
        true
    );
    let before = count(&tx, "audit_log");
    tx.execute_batch("CREATE TEMP TRIGGER reject_chart_audit BEFORE INSERT ON audit_log WHEN NEW.event_type IN ('account_created','account_role_confirmed') BEGIN SELECT RAISE(ABORT,'synthetic audit refusal'); END;").unwrap();
    assert_eq!(add(&tx, "B", "asset")["ok"], false);
    assert_eq!(
        execute(&tx, Operation::RoleConfirm, &role, "user:second").unwrap()["ok"],
        false
    );
    assert_eq!(count(&tx, "accounts"), 1);
    assert_eq!(count(&tx, "account_role_mappings"), 1);
    assert_eq!(count(&tx, "audit_log"), before);
    assert_eq!(source_resolution(&tx, "bank").unwrap()["version"], 1);
    tx.commit().unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM accounts", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
