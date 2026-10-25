use super::*;
#[test]
fn original123_structural_queries_and_exact_decoding() {
    for q in [
        "query=%FF",
        "query=%",
        "query=x&query=y",
        "cursor=1&cursor=2",
        "unknown=x",
        "query",
        "&",
        "query=x&&limit=1",
    ] {
        assert!(fields(q).is_none(), "{q}");
    }
    assert_eq!(
        fields("query=%F0%9F%98%80+a&cursor=18446744073709551615").unwrap()["query"],
        "😀 a"
    );
    assert_eq!(unsigned::<u64>("18446744073709551615"), Some(u64::MAX));
    for s in ["-1", "+1", "1.0", "null", ""] {
        assert!(unsigned::<u64>(s).is_none());
    }
    assert_eq!(
        decode("unknown%2F..%2F%F0%9F%98%80", false).unwrap(),
        "unknown/../😀"
    );
}
#[tokio::test]
async fn original123_http_public_inert_errors_and_native_preserved() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, super::super::router(false))
            .await
            .unwrap();
    });
    let client = reqwest::Client::new();
    for (path, status) in [
        ("/api/original/agent-about", 200),
        ("/api/original/agent-capabilities?limit=50", 200),
        (
            "/api/original/agent-capabilities?cursor=18446744073709551615",
            200,
        ),
        ("/api/original/agent-capabilities?query=%FF", 400),
        ("/api/original/agent-capabilities?limit=1&limit=1", 400),
        ("/api/original/agent-workflows/unknown%2Fid", 404),
        ("/api/original/agent-workflows/company-workspace-setup", 200),
    ] {
        let response = client
            .get(format!("http://{addr}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), status, "{path}");
        let body: Value = response.json().await.unwrap();
        if status == 404 {
            assert_eq!(body["code"], "UNKNOWN_WORKFLOW");
        }
    }
    let original: Value = client
        .get(format!("http://{addr}/api/original/agent-about"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(original["catalogue"]["workflowCount"], 31);
    assert_eq!(original["workspaceConnected"], false);
    let native: Value = client
        .get(format!("http://{addr}/api/agent-about"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(native["catalogue"]["workflowCount"], 2);
    assert_eq!(
        client
            .get(format!("http://{addr}/api/original/agent-about"))
            .body("{}")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        client
            .head(format!("http://{addr}/api/original/agent-about"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    task.abort();
}
