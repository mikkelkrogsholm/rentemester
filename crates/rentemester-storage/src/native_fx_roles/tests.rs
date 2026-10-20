use super::*;
use rusqlite::{Connection, TransactionBehavior};
fn fixture() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    let mut d = Connection::open_in_memory().unwrap();
    crate::fresh::initialize_empty_pair(&mut c, &mut d, "Synthetic FX roles").unwrap();
    d.execute_batch("INSERT INTO accounts(account_no,name,type,normal_balance)VALUES('GAIN','Synthetic gain','income','credit'),('GAIN2','Synthetic second gain','income','credit'),('LOSS','Synthetic loss','expense','debit'),('ASSET','Synthetic asset','asset','debit');").unwrap();
    d
}
#[test]
fn additive_confirmation_rollback_and_append_only_history() {
    let mut d = fixture();
    let base = schema(&d).unwrap();
    {
        let tx = d
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        assert!(resolve(&tx, "fx_gain").unwrap().is_none());
        confirm(&tx, "fx_gain", "GAIN", "synthetic").unwrap();
        assert_eq!(resolve(&tx, "fx_gain").unwrap().unwrap().0, "GAIN");
        assert_eq!(strip_schema(&tx, &schema(&tx).unwrap()).unwrap(), base);
        tx.rollback().unwrap();
    }
    assert_eq!(schema(&d).unwrap(), base);
    let tx = d
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    confirm(&tx, "fx_gain", "GAIN", "synthetic").unwrap();
    confirm(&tx, "fx_gain", "GAIN", "synthetic").unwrap();
    confirm(&tx, "fx_gain", "GAIN2", "synthetic").unwrap();
    confirm(&tx, "fx_loss", "LOSS", "synthetic").unwrap();
    assert_eq!(resolve(&tx, "fx_gain").unwrap().unwrap().1, 2);
    assert!(
        tx.execute(
            "UPDATE native_fx_role_confirmations SET account_no='GAIN'",
            []
        )
        .is_err()
    );
    assert!(
        tx.execute("DELETE FROM native_fx_role_confirmations", [])
            .is_err()
    );
    tx.commit().unwrap();
    validate_history(&d).unwrap();
}
#[test]
fn partial_unknown_objects_and_identity_damage_are_refused() {
    let d = fixture();
    d.execute_batch("CREATE TABLE native_fx_role_schema(id INTEGER PRIMARY KEY);")
        .unwrap();
    assert!(strip_schema(&d, &schema(&d).unwrap()).is_err());
    let d = fixture();
    d.execute_batch(DDL).unwrap();
    d.execute(
        "INSERT INTO native_fx_role_schema VALUES(1,1,?1)",
        [identity()],
    )
    .unwrap();
    d.execute_batch("CREATE TABLE native_fx_role_foreign(value TEXT);")
        .unwrap();
    assert!(validate_history(&d).is_err());
    let d = fixture();
    d.execute_batch(DDL).unwrap();
    d.execute("INSERT INTO native_fx_role_schema VALUES(1,1,'wrong')", [])
        .unwrap();
    assert!(validate_history(&d).is_err());
}
#[test]
fn inactive_account_preserves_history_but_cannot_resolve_or_confirm() {
    let mut d = fixture();
    let tx = d
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .unwrap();
    confirm(&tx, "fx_gain", "GAIN", "synthetic").unwrap();
    tx.execute("UPDATE accounts SET active=0 WHERE account_no='GAIN'", [])
        .unwrap();
    validate_history(&tx).unwrap();
    assert!(resolve(&tx, "fx_gain").is_err());
    assert!(confirm(&tx, "fx_gain", "ASSET", "synthetic").is_err());
    tx.rollback().unwrap();
}
#[test]
fn noncontiguous_versions_or_missing_historical_accounts_are_refused() {
    for (version, account_id) in [(2, 1), (1, 99999)] {
        let d = fixture();
        d.pragma_update(None, "foreign_keys", "OFF").unwrap();
        d.execute_batch(DDL).unwrap();
        d.execute(
            "INSERT INTO native_fx_role_schema VALUES(1,1,?1)",
            [identity()],
        )
        .unwrap();
        d.execute("INSERT INTO native_fx_role_confirmations(role,account_id,account_no,version,confirmed_by,confirmation_source)VALUES('fx_gain',?1,'GAIN',?2,'user:synthetic','explicit')",params![account_id,version]).unwrap();
        assert!(validate_history(&d).is_err());
    }
}
