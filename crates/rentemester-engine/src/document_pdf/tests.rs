use super::*;
use serde_json::json;
fn identity() -> ParserIdentity {
    ParserIdentity {
        parser_id: "pdfium".into(),
        parser_version: "synthetic-engine.adapter1".into(),
        contract_version: NATIVE_CONTRACT.into(),
    }
}
#[test]
fn shape_bounds_and_unsupported_unicode_are_explicit() {
    let raw = json!({"contractVersion":NATIVE_CONTRACT,"inputSha256":"a".repeat(64),"status":"no_text_layer","errorCode":"no_text_layer","pages":[{"pageNumber":1,"width":612,"height":792,"rotation":0,"text":"","layout":[]}]});
    let mut r = decode(&serde_json::to_vec(&raw).unwrap()).unwrap();
    r.validate(&"a".repeat(64), &identity()).unwrap();
    r.pages[0].rotation = 45;
    assert!(r.validate(&"a".repeat(64), &identity()).is_err());
    r.pages[0].rotation = 0;
    r.pages[0].width = f64::NAN;
    assert!(r.validate(&"a".repeat(64), &identity()).is_err());
    assert_eq!(
        decode(br#"{"contractVersion":"\ud800"}"#).unwrap_err().code,
        "PDF_EVIDENCE_UNSUPPORTED_UNICODE"
    );
}
