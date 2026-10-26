use super::*;
fn input(v: &Value) -> Value {
    let mut v = v.clone();
    for key in ["actor", "principal", "expectedVersion", "expectedEventHash"] {
        v.as_object_mut().unwrap().remove(key);
    }
    v
}
#[test]
fn actual_main_page_relation_hashes_render_and_refusals() {
    let v: Value = serde_json::from_str(include_str!("main-pages.json")).unwrap();
    let mut prior = None;
    for name in ["first", "second", "target"] {
        let row = &v[name];
        let i: PageInput = serde_json::from_value(input(&row["input"])).unwrap();
        let p = page(
            &i,
            "unused-generated",
            if name == "second" {
                prior.as_ref()
            } else {
                None
            },
            "user:synthetic",
            "synthetic-member",
            row["at"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(p.output(true).unwrap(), row["result"], "{name}");
        if name == "first" {
            prior = Some(p)
        }
    }
    let mut prior = None;
    for name in ["relation", "rel2"] {
        let row = &v[name];
        let i: RelationInput = serde_json::from_value(input(&row["input"])).unwrap();
        let r = relation(
            &i,
            "unused-generated",
            prior.as_ref(),
            "user:synthetic",
            "synthetic-member",
            row["at"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(r.output().unwrap(), row["result"], "{name}");
        prior = Some(r);
    }
    for row in v["rendered"].as_array().unwrap() {
        let r = render(row["markdown"].as_str().unwrap());
        if row["refused"] == true {
            assert!(r.is_err(), "{}", row["markdown"])
        } else {
            assert_eq!(json!(r.unwrap()), row["result"], "{}", row["markdown"]);
        }
    }
    assert!(!safe_url("https://user:secret@example.invalid"));
    assert!(!safe_url("%6aavascript:evil"));
    assert!(
        Scope::Company {
            company_slug: "bad/path".into()
        }
        .validate()
        .is_err()
    );
}
