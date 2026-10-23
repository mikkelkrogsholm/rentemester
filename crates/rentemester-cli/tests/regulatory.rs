use std::process::Command;
const BIN:&str=env!("CARGO_BIN_EXE_rentemester-rust");
#[test]
fn static_reg_never_requires_company_or_credentials_even_when_engine_unavailable(){
    for command in ["coverage","citations"] {
        let out=Command::new(BIN).args(["reg",command,"--json"]).output().unwrap();
        let text=String::from_utf8(out.stdout).unwrap();
        assert!(!text.contains("company required"));
        assert!(!text.contains("cookie-file required"));
        assert!(!text.contains("unknown command"));
        // Component scaffold is NOT evidence that the final corpus is correct.
        assert!(!text.is_empty());
    }
}
