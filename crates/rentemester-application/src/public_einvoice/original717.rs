//! Company secret capabilities are obtained only inside current admitted Owner.
use super::*;
pub fn configure(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    body: &Value,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyAdmin,
        |tx, actor| {
            if body["confirm"] != true {
                return Ok(json!({"ok":false,"errors":["confirm: true is required"]}));
            }
            let secret = json!({"apiLicenseKey":body["apiLicenseKey"],"environment":body.get("environment").cloned().unwrap_or(json!("test"))});
            let bytes = serde_json::to_vec(&secret).map_err(|_| refused())?;
            rentemester_runtime::public_einvoice::original717::from_admitted_secret(&bytes)
                .map_err(|_| refused())?;
            guard
                .write_digisense_config(&bytes)
                .map_err(|_| refused())?;
            let generation = rentemester_runtime::auth::new_token().map_err(|_| refused())?;
            tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor) VALUES('original717_digisense_config','private_config','digisense',?,?)",rusqlite::params![json!({"generation":generation.as_str(),"sha256":rentemester_storage::invoice_email::sha(&bytes)}).to_string(),actor]).map_err(|_|refused())?;

            Ok(
                json!({"ok":true,"configPath":"config/digisense.json","environment":secret["environment"]}),
            )
        },
    )
}
/// Never returned through HTTP; caller constructs a fresh immutable provider client.
pub fn secret(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
) -> Result<Committed<Option<(Vec<u8>, String)>>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        CookieAuthorization {
            cookie: context.cookie,
            clock: context.clock,
            requirement: SessionRequirement::Fresh,
        },
        selection(&context),
        Permission::CompanyExternalSend,
        |tx, _| {
            use rusqlite::OptionalExtension;
            let generation:Option<String>=tx.query_row("SELECT message FROM audit_log WHERE event_type='original717_digisense_config' AND entity_type='private_config' AND entity_id='digisense' ORDER BY id DESC LIMIT 1",[],|r|r.get(0)).optional().map_err(|_|refused())?;
            let bytes = guard.read_digisense_config().map_err(|_| refused())?;
            match (bytes, generation) {
                (Some(bytes), Some(generation)) => {
                    let proof: Value = serde_json::from_str(&generation).map_err(|_| refused())?;
                    if proof["sha256"] != rentemester_storage::invoice_email::sha(&bytes) {
                        return Err(refused());
                    }
                    Ok(Some((bytes, generation)))
                }
                (None, None) => Ok(None),
                _ => Err(refused()),
            }
        },
    )
}
