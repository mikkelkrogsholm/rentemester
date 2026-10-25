use super::*;
impl ApiClient {
    /// Source-flat shape, distinct from native registration operation receipts.
    pub fn backup_views_original717_schema(name: &str) -> Value {
        let mut p = json!({});
        let mut required = vec![];
        match name {
            "system_backup_destination_add" => {
                for k in ["label", "kind", "location", "attestedBy"] {
                    p[k] = json!({"type":"string","minLength":1});
                    required.push(k);
                }
                p["kind"] = json!({"type":"string","enum":["local-folder","dropbox","google-drive","ssh","other"]});
                p["inEeaOrEu"] = json!({"type":"boolean"});
                required.push("inEeaOrEu");
                for k in ["regionCountry", "regionNote", "itSecurityNote", "at"] {
                    p[k] = json!({"type":"string"});
                }
                for k in ["nonRelatedParty", "itSecurityMeetsStandards"] {
                    p[k] = json!({"type":"boolean"});
                }
            }
            "system_backup_destination_remove" => {
                p["id"] = json!({"type":"string","minLength":1});
                required.push("id");
            }
            "system_backup_confirm_placement" => {
                for k in ["destinationId", "backupId", "archiveSha256"] {
                    p[k] = json!({"type":"string","minLength":1});
                    required.push(k);
                }
                p["archiveSizeBytes"] = json!({"type":"number"});
                p["actorKind"] = json!({"type":"string","enum":["human","agent"]});
                for k in ["at", "note"] {
                    p[k] = json!({"type":"string"});
                }
            }
            "system_backup_governance" => {
                p["asOf"] = json!({"type":"string"});
            }
            "system_backup_destination_list" => {}
            _ => return Value::Null,
        }
        if !matches!(
            name,
            "system_backup_governance" | "system_backup_destination_list"
        ) {
            p["confirm"] = json!({"type":"boolean"});
        }
        object(p, &required)
    }
    pub async fn backup_views_original717(
        &self,
        name: &str,
        company: &str,
        input: Value,
    ) -> Result<Value, Error> {
        let route = match name {
            "system_backup_destination_list" => "original-system-backup-destination-list",
            "system_backup_destination_add" => "original-system-backup-destination-add",
            "system_backup_destination_remove" => "original-system-backup-destination-remove",
            "system_backup_confirm_placement" => "original-system-backup-confirm-placement",
            "system_backup_governance" => "original-system-backup-governance",
            _ => return Err(Error::InvalidInput("unknown backup source operation")),
        };
        let bytes = serde_json::to_vec(&input)
            .map_err(|_| Error::InvalidInput("invalid backup source input"))?;
        if bytes.len() > 16384 {
            return Err(Error::InvalidInput("backup source input exceeds16KiB"));
        }
        body(&bytes, &Self::backup_views_original717_schema(name))?;
        let read = matches!(
            name,
            "system_backup_destination_list" | "system_backup_governance"
        );
        if !read && input["confirm"] != true {
            return Ok(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":["confirm: true required for write tool"],"appliedRules":["DK-BOOKKEEPING-BACKUP-001"]}),
            );
        }
        let company = self.diagnostics_company_original118(company)?;
        let query = input["asOf"]
            .as_str()
            .map(|s| vec![("asOf", s)])
            .unwrap_or_default();
        let wire = self
            .request(
                if read { Method::GET } else { Method::POST },
                &company_path(&company, route)?,
                &query,
                if read { None } else { Some(bytes) },
            )
            .await?;
        let c = &wire["result"];
        if wire["ok"] != true || !c["ok"].is_boolean() {
            return Err(Error::Protocol);
        }
        if c["ok"] == false {
            if !c["errors"].as_array().is_some_and(|a| {
                (name == "system_backup_governance" || !a.is_empty())
                    && a.iter().all(Value::is_string)
            }) {
                return Err(Error::Protocol);
            }
        } else {
            match name {
                "system_backup_destination_list" => {
                    if !c["destinations"].is_array()
                        || c["destinationCount"].as_u64()
                            != c["destinations"].as_array().map(|a| a.len() as u64)
                    {
                        return Err(Error::Protocol);
                    }
                }
                "system_backup_destination_add" => {
                    if !c["destination"]["id"].is_string()
                        || !c["destination"]["placements"].is_array()
                    {
                        return Err(Error::Protocol);
                    }
                }
                "system_backup_confirm_placement" => {
                    if !c["placement"]["verified"].is_boolean()
                        || !c["placement"]["archiveSha256"].is_string()
                    {
                        return Err(Error::Protocol);
                    }
                }
                "system_backup_governance" => {
                    if !c["compliance"].is_object()
                        || !c["lock"].is_object()
                        || !c["latestBackupPlacedOffsite"].is_boolean()
                        || !c["destinations"].is_array()
                    {
                        return Err(Error::Protocol);
                    }
                }
                _ => {}
            }
        }
        Ok(c.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original717_backup_sdk_flat_contract_and_confirmation_without_transport() {
        let add = ApiClient::backup_views_original717_schema("system_backup_destination_add");
        assert!(body(br#"{"label":"Synthetic","kind":"other","location":"opaque:synthetic","attestedBy":"Synthetic operator","inEeaOrEu":false}"#,&add).is_ok());
        for wire in [
            br#"{"id":"synthetic","confirm":"true"}"#.as_slice(),
            br#"{"id":"synthetic","operationId":"caller-operation"}"#,
        ] {
            assert!(
                body(
                    wire,
                    &ApiClient::backup_views_original717_schema("system_backup_destination_remove")
                )
                .is_err()
            );
        }
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let api = ApiClient::with_service_key("http://127.0.0.1:9", true, "synthetic".into())
                .unwrap();
            let r = api
                .backup_views_original717(
                    "system_backup_destination_remove",
                    "synthetic",
                    json!({"id":"synthetic"}),
                )
                .await
                .unwrap();
            assert_eq!(r["code"], "CONFIRM_REQUIRED");
        });
    }
}
