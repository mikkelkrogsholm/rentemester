//! Legacy /api/rules projection; reuse the regulatory rule parser, not coverage status.
use super::*;

// Match the original shallow section reader, including its DK-only source list.
// EU-GDPR remains in the catalog and rules, but not in a bundle's sources list.
fn section_values(text: &str, section: &str) -> Vec<String> {
    let mut active = false;
    let mut values = vec![];
    for raw in text.lines() {
        if raw.split_once(':').is_some_and(|(key, _)| {
            !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        }) {
            active = raw.trim() == format!("{section}:");
            continue;
        }
        if !active {
            continue;
        }
        let Some(item) = raw.trim().strip_prefix('-').map(str::trim_start) else {
            continue;
        };
        let value = match section {
            "sources"
                if item.strip_prefix("DK-").is_some_and(|suffix| {
                    !suffix.is_empty()
                        && suffix
                            .chars()
                            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
                }) =>
            {
                Some(item)
            }
            "vat_codes" => item.strip_prefix("code:").map(str::trim),
            _ => None,
        };
        if let Some(value) = value.filter(|v| !v.is_empty() && !v.chars().any(char::is_whitespace))
        {
            values.push(value.to_owned());
        }
    }
    values
}

pub(super) fn compute(catalog: &str, bundles: &[(&str, &str)]) -> Result<Value, Error> {
    let legal_sources: Value =
        serde_json::from_str(catalog).map_err(|_| invalid("malformed catalog"))?;
    let sources = legal_sources
        .as_array()
        .ok_or_else(|| invalid("catalog must be array"))?;
    for source in sources {
        for field in ["id", "title", "authority", "category", "url"] {
            if !source[field].is_string() {
                return Err(invalid("malformed legal source"));
            }
        }
        for field in ["xmlUrl", "notes"] {
            if source.get(field).is_some_and(|v| !v.is_string()) {
                return Err(invalid("malformed optional legal source field"));
            }
        }
    }
    let mut rule_bundles = vec![];
    let mut rules = vec![];
    for (name, text) in bundles {
        let (version, parsed) = parsers::parse_rules(text, name)?;
        rule_bundles.push(
            json!({"name":name,"version":version,"ruleCount":parsed.len(),
            "sources":section_values(text,"sources"),"vatCodes":section_values(text,"vat_codes")}),
        );
        rules.extend(parsed);
    }
    rule_bundles.sort_by_cached_key(|b| s(b, "name").to_owned());
    rules.sort_by_cached_key(|r| s(r, "ruleId").to_owned());
    Ok(json!({"ok":true,"ruleBundles":rule_bundles,"rules":rules,"legalSources":legal_sources}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_bundle_counts_and_versions() {
        let metadata = bundled_rules_metadata().unwrap();
        let bundles = metadata["ruleBundles"].as_array().unwrap();
        // Counts/versions independently observed from the original TypeScript reader.
        let expected = [
            ("annual-report", "0.0.1", 2),
            ("bank", "0.0.2", 1),
            ("bookkeeping", "0.0.8", 21),
            ("documents", "0.0.3", 5),
            ("email", "0.0.2", 1),
            ("gdpr", "0.0.1", 2),
            ("invoices", "0.0.11", 35),
            ("mail-intake", "0.0.3", 3),
            ("mileage", "0.0.2", 1),
            ("peppol", "0.0.4", 1),
            ("tax-return", "0.0.1", 1),
            ("vat", "0.0.5", 11),
        ];
        assert_eq!(bundles.len(), 12);
        let rules = metadata["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 84);
        assert!(
            rules
                .windows(2)
                .all(|pair| s(&pair[0], "ruleId") < s(&pair[1], "ruleId"))
        );
        for (bundle, (name, version, count)) in bundles.iter().zip(expected) {
            assert_eq!(bundle["name"], name);
            assert_eq!(bundle["version"], format!("dk-{name}-v{version}"));
            assert_eq!(bundle["ruleCount"], count);
            assert_eq!(
                rules.iter().filter(|r| r["bundle"] == name).count(),
                count as usize
            );
            assert_eq!(bundle.as_object().unwrap().len(), 5);
        }
        assert_eq!(metadata["ok"], true);
        assert_eq!(metadata.as_object().unwrap().len(), 4);
    }

    #[test]
    fn original_sources_vat_codes_and_optional_catalog_fields() {
        let metadata = bundled_rules_metadata().unwrap();
        let bundles = metadata["ruleBundles"].as_array().unwrap();
        assert_eq!(
            bundles[5]["sources"],
            json!(["DK-BOGFORINGSLOVEN-2022-700"])
        );
        assert_eq!(
            bundles[11]["vatCodes"],
            json!([
                "DK_PURCHASE_25",
                "DK_SALE_25",
                "EU_SERVICE_REVERSE_CHARGE",
                "NON_EU_SERVICE_REVERSE_CHARGE",
                "REPRESENTATION_SPECIAL",
                "DK_BAD_DEBT_25",
                "DK_SALE_EXEMPT",
                "REVERSE_CHARGE_EXEMPT",
                "DOMESTIC_REVERSE_CHARGE_EXEMPT",
                "OSS_EU_CONSUMER"
            ])
        );
        assert!(bundles[..11].iter().all(|b| b["vatCodes"] == json!([])));
        let sources = metadata["legalSources"].as_array().unwrap();
        assert_eq!(sources.len(), 12);
        let gdpr = sources
            .iter()
            .find(|s| s["id"] == "EU-GDPR-2016-679")
            .unwrap();
        assert!(gdpr.get("xmlUrl").is_none());
        assert!(gdpr["notes"].is_string());
        assert_eq!(
            sources[0]["xmlUrl"],
            "https://www.retsinformation.dk/eli/lta/2022/700/xml"
        );
        let rules = metadata["rules"].as_array().unwrap();
        assert_eq!(
            rules
                .iter()
                .filter(|r| r["sourceId"] == "EU-GDPR-2016-679")
                .count(),
            2
        );
        for rule in rules {
            assert_eq!(rule.as_object().unwrap().len(), 8);
            for field in [
                "ruleId",
                "bundle",
                "sourceId",
                "name",
                "explanation",
                "severity",
                "category",
            ] {
                assert!(rule[field].is_string());
            }
            for citation in rule["provisions"].as_array().unwrap() {
                assert_eq!(citation.as_object().unwrap().len(), 2);
                assert!(citation["ref"].is_string());
                assert!(citation["textHash"].is_string());
            }
        }
    }

    #[test]
    fn shallow_headers_do_not_collect_rule_sources_or_non_dk_sources() {
        let text = "sources:\n  - DK-ABC-123\n  - EU-GDPR-2016-679\n  - DK-lower\n  - DK-ABC extra\nvat_codes:\n  - code: VAT_ONE\nrules:\n  - rule_id: R\n    source_id: DK-NOT-A-HEADER\n";
        assert_eq!(section_values(text, "sources"), vec!["DK-ABC-123"]);
        assert_eq!(section_values(text, "vat_codes"), vec!["VAT_ONE"]);
    }

    #[test]
    fn malformed_inputs_fail_closed_without_partial_metadata() {
        assert!(compute("{}", &[]).is_err());
        assert!(compute("[{}]", &[]).is_err());
        assert!(compute("[]", &[("bad", "rules:\n")]).is_err());
        let malformed = "version: v1\nrules:\n  - rule_id: BAD\n    provisions:\n      - ref: citation without hash\n";
        assert!(compute("[]", &[("bad", malformed)]).is_err());
        let minimal = compute(
            r#"[{"id":"X","title":"T","authority":"A","category":"C","url":"U"}]"#,
            &[],
        )
        .unwrap();
        assert!(minimal["legalSources"][0].get("notes").is_none());
        assert!(minimal["legalSources"][0].get("xmlUrl").is_none());
    }
}
