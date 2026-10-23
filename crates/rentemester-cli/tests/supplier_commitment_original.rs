use std::process::Command;
const BIN:&str=env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn seven_original_commands_discover_without_opening_credentials() {
    for op in ["plan","apply","list","change","match","matches","alerts"] {
        let output=Command::new(BIN).args(["--company=synthetic","supplier-commitment",op,"--help","--cookie-file=/not/opened"]).output().unwrap();
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        let text=String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("INLINE_JSON"));
        assert!(text.contains("NOT offline pure TypeScript"));
    }
}
#[test]
fn usage_and_confirmation_fail_before_connection_or_credentials() {
    for args in [
        vec!["supplier-commitment","unknown"],
        vec!["supplier-commitment","list","--actor=forged"],
        vec!["supplier-commitment","alerts"],
        vec!["supplier-commitment","plan","--input=fixture.json"],
        vec!["supplier-commitment","change","--id=synthetic","--action=paused","--reason=reviewed","--confirm=no"],
        vec!["supplier-commitment","change","--id=synthetic","--action=paused","--reason=reviewed"],
        vec!["supplier-commitment","match","--input={\"commitmentId\":\"synthetic\",\"occurrenceDate\":\"2026-01-01\",\"evidence\":{\"kind\":\"payable\",\"id\":\"1\",\"amount\":1}}","--confirm=yes"],
        vec!["supplier-commitment","list","--id=forbidden"],
    ] {
        let output=Command::new(BIN).args(args).args(["--server=http://127.0.0.1:1","--allow-loopback-http","--company=synthetic","--cookie-file=/not/opened"]).output().unwrap();
        assert_eq!(output.status.code(),Some(2),"{}",String::from_utf8_lossy(&output.stdout));
        assert!(!String::from_utf8_lossy(&output.stderr).contains("credential"));
    }
}
