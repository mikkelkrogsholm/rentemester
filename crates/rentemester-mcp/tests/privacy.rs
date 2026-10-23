use serde_json::Value;
use std::process::Command;
const BIN:&str=env!("CARGO_BIN_EXE_rentemester-mcp");
#[test]
fn privacy_catalogue_is_confirmed_audited_and_strict(){
    let out=Command::new(BIN).arg("--catalogue").output().unwrap();
    assert!(out.status.success());
    let v:Value=serde_json::from_slice(&out.stdout).unwrap();
    for name in ["gdpr_discover","gdpr_forget","gdpr_legal_hold","gdpr_audit_log"] {
        let t=v["tools"].as_array().unwrap().iter().find(|t|t["name"]==name).unwrap();
        assert_eq!(t["annotations"]["readOnlyHint"],false);
        assert_eq!(t["annotations"]["idempotentHint"],false);
        assert_eq!(t["inputSchema"]["additionalProperties"],false);
        assert_eq!(t.pointer("/inputSchema/properties/confirm/const").unwrap(),true);
        assert_eq!(t.pointer("/inputSchema/properties/body/contentSchema/additionalProperties").unwrap(),false);
    }
    assert!(!v["tools"].as_array().unwrap().iter().any(|t|matches!(t["name"].as_str(),Some("reg_coverage"|"reg_citations"|"compliance_report"))));
}
