use super::*;
fn proposal() -> Value {
    json!({"predicate":"business_description","value":"Synthetic scope","source":{"kind":"user","ref":"synthetic"},"validFrom":"2026-01-01"})
}
#[test]
fn confirmed_knowledge_schemas_refuse_caller_attribution_at_every_depth() {
    let propose = proposal();
    let review =
        json!({"assertionId":"synthetic-id","decision":"approved","reason":"Synthetic review"});
    let supersede = json!({"assertionId":"synthetic-id","replacement":proposal()});
    for (operation, mut body) in [
        ("propose", propose),
        ("review", review),
        ("supersede", supersede),
    ] {
        let path = format!("/api/companies/a/knowledge/{operation}");
        body["confirm"] = json!(true);
        assert!(parse_mutation(&path, &serde_json::to_vec(&body).unwrap()).is_ok());
        for field in [
            "actor",
            "principal",
            "principalKind",
            "principalId",
            "companySlug",
        ] {
            let mut forged = body.clone();
            forged[field] = json!("user:workspace-owner");
            assert!(
                parse_mutation(&path, &serde_json::to_vec(&forged).unwrap()).is_err(),
                "{operation}/{field}"
            );
        }
        body["confirm"] = json!(false);
        assert_eq!(
            parse_mutation(&path, &serde_json::to_vec(&body).unwrap())
                .err()
                .unwrap()
                .1,
            "confirmation_required"
        );
    }
    for field in ["actor", "principal", "companySlug"] {
        let mut nested = proposal();
        nested[field] = json!("user:workspace-owner");
        let body = json!({"confirm":true,"assertionId":"synthetic-id","replacement":nested});
        assert!(
            parse_mutation(
                "/api/companies/a/knowledge/supersede",
                &serde_json::to_vec(&body).unwrap()
            )
            .is_err()
        );
    }
}
