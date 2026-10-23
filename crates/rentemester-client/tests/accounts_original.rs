use rentemester_client::{
    AccountsOriginalOperation as Op, ApiClient, Error, original_accounts_schema,
    validate_original_accounts,
};
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};
fn add() -> Value {
    json!({"input":{"accountNo":" 1790 ","name":" Synthetic bank ","type":" asset ","normalBalance":" ","defaultVatCode":"","allowDirectPosting":false},"confirm":true})
}
#[test]
fn accounts_original_strict_input_and_discovery() {
    for op in [Op::List, Op::Add, Op::RolesStatus, Op::RoleConfirm] {
        assert!(original_accounts_schema(op)["required"].is_array());
    }
    assert!(validate_original_accounts(Op::Add, &add()).is_ok());
    for (key, value) in [
        ("type", json!("other")),
        ("accountNo", json!("\n1790")),
        ("name", json!(" ")),
        ("normalBalance", json!("other")),
        ("allowDirectPosting", json!("false")),
        ("createdByProgram", json!("caller")),
    ] {
        let mut v = add();
        v["input"][key] = value;
        assert!(validate_original_accounts(Op::Add, &v).is_err(), "{key}");
    }
    assert!(
        validate_original_accounts(
            Op::RoleConfirm,
            &json!({"role":"fx_gain","accountNo":"1790","confirm":true})
        )
        .is_err()
    );
    assert!(validate_original_accounts(Op::List, &json!({"actor":"user:x"})).is_err());
}
fn server(status: u16, v: Value) -> (String, thread::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", l.local_addr().unwrap());
    let t = thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = vec![];
        loop {
            let mut b = [0; 4096];
            let n = s.read(&mut b).unwrap();
            assert!(n > 0);
            bytes.extend_from_slice(&b[..n]);
            if let Some(i) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&bytes[..i]);
                let length: usize = header
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                if bytes.len() >= i + 4 + length {
                    break;
                }
            }
        }
        let text = v.to_string();
        write!(
            s,
            "HTTP/1.1 {status} Mock\r\nContent-Length:{}\r\nConnection:close\r\n\r\n{text}",
            text.len()
        )
        .unwrap();
        String::from_utf8(bytes).unwrap()
    });
    (origin, t)
}
#[test]
fn accounts_original_status_first_bound_receipts_and_order() {
    tokio::runtime::Runtime::new().unwrap().block_on(async{
    let good=json!({"ok":true,"accountNo":"1790","appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"],"errors":[]});
    for (status,v,expected) in [(200,good.clone(),None),(403,good.clone(),Some(Error::Refused{status:403})),(503,good.clone(),Some(Error::OutcomeUnknown)),(200,json!({"ok":true,"accountNo":"other","appliedRules":["DK-CHART-OF-ACCOUNTS-ADD-001"],"errors":[]}),Some(Error::OutcomeUnknown)),(200,json!({"ok":false,"errors":["refused"]}),Some(Error::OutcomeUnknown)),(422,json!({"ok":false,"errors":["duplicate account"]}),None)]{
        let(origin,t)=server(status,v.clone());let api=ApiClient::new(&origin,true,None).unwrap();let result=api.original_accounts("synthetic",Op::Add,&add(),Some("service_account:owner"),None).await;
        if let Some(e)=expected{assert_eq!(result,Err(e));}else{assert_eq!(result.unwrap(),v);}let request=t.join().unwrap();assert!(request.starts_with("POST /api/companies/synthetic/original/accounts/add "));assert!(request.contains("service_account:owner"));
    }
    for v in [json!({"ok":true,"count":1,"rows":[]}),json!({"ok":true,"count":2,"rows":[{"account_no":"2","name":"Two","type":"asset","default_vat_code":null},{"account_no":"1","name":"One","type":"asset","default_vat_code":null}]})]{let(origin,t)=server(200,v);let api=ApiClient::new(&origin,true,None).unwrap();assert_eq!(api.original_accounts("synthetic",Op::List,&json!({}),None,None).await,Err(Error::Protocol));t.join().unwrap();}
});
}

#[test]
fn accounts_original_retained_native_seed_and_typed_history_are_readable() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let roles = ["bank", "debtors", "creditors", "output_vat", "input_vat", "reverse_charge_vat", "vat_settlement", "operational_default"];
        for actor in ["system", "agent:retained", "system:retained", "user:old", "service_account:old", "forged", "agent:", "user:bad actor"] {
            let resolutions: Vec<_> = roles.iter().map(|role| json!({"ok":true,"role":role,"accountNo":"1790","version":1,"confirmedBy":actor,"confirmationSource":if actor == "system" {"native_seed"} else {"explicit"}})).collect();
            let v = json!({"ok":true,"status":"complete","missing":[],"ambiguous":[],"proposals":[],"candidates":[],"reasons":[],"roles":resolutions});
            let (origin,t) = server(200,v.clone());
            let api=ApiClient::new(&origin,true,None).unwrap();
            let result=api.original_accounts("synthetic",Op::RolesStatus,&json!({}),None,None).await;
            if matches!(actor,"forged"|"agent:"|"user:bad actor") {assert_eq!(result,Err(Error::Protocol));} else {assert_eq!(result.unwrap(),v);}
            assert!(t.join().unwrap().starts_with("GET /api/companies/synthetic/original/accounts/roles-status "));
        }
    });
}
