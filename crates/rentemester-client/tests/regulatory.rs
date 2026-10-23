use rentemester_client::{ApiClient,Error};
#[tokio::test]
async fn compliance_date_is_strict_before_network(){
    let api=ApiClient::new("http://127.0.0.1:9",true,Some("session=synthetic".into())).unwrap();
    for date in ["2026-02-30","2026-1-01","0000-01-01","2026-01-01&actor=other"] {
        assert!(matches!(api.compliance_report("synthetic",Some(date)).await,Err(Error::InvalidInput(_))));
    }
}
