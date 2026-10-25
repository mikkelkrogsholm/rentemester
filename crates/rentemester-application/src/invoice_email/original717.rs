//! Original built-in dry-run operation; native live dispatch remains separate.
use super::*;
use rentemester_runtime::invoice_mail::original717::BuiltIn;
use rusqlite::OptionalExtension;
fn result(error: &str) -> Value {
    json!({"ok":false,"appliedRules":["DK-EMAIL-DELIVERY-001"],"errors":[error]})
}
pub fn send_original717(
    control: &mut Connection,
    company: &mut Connection,
    guard: &mut NamespaceGuard,
    context: Context<'_>,
    body: &Value,
) -> Result<Committed<Value>, Failure> {
    control_fence::execute_cookie_subject(
        control,
        company,
        authorization(&context),
        context.selection,
        Permission::CompanyExternalSend,
        |tx, actor| {
            if body["confirm"] != true {
                return Ok(result("confirm: true is required"));
            }
            let id = match rentemester_storage::invoice_lifecycle::resolve_invoice(
                tx,
                body,
                "documentId",
                "invoiceNumber",
            ) {
                Ok(id) => id,
                Err(e) => return Ok(result(&e.message)),
            };
            let Some(bytes) = guard.read_smtp_config().map_err(|_| store::refused())? else {
                return Ok(result(
                    "missing SMTP config: create config/smtp.json (host, port, fromAddress) in the company directory",
                ));
            };
            let transport = match BuiltIn::from_admitted_config(&bytes) {
                Ok(t) => t,
                Err(_) => return Ok(result("SMTP config is required (host, port, fromAddress)")),
            };
            let (no,buyer): (String,Option<String>) = tx.query_row("SELECT invoice_no,recipient_name FROM documents WHERE id=? AND document_type='issued_invoice'",[id],|r|Ok((r.get(0)?,r.get(1)?))).map_err(|_|store::refused())?;
            let to = match body["to"].as_str().filter(|s|!s.trim().is_empty()) {
            Some(s)=>Some(s.trim().to_owned()),
            None=>tx.query_row("SELECT email FROM customers WHERE name=? AND archived=0 AND email IS NOT NULL AND TRIM(email)<>'' ORDER BY id DESC LIMIT 1",[buyer.unwrap_or_default().trim()],|r|r.get::<_,String>(0)).optional().map_err(|_|store::refused())?,
        };
            let Some(to) = to else {
                return Ok(result(&format!(
                    "no recipient email for invoice {no}: pass an explicit recipient or set the customer email"
                )));
            };
            let input = Send {
                invoice_document_id: id,
                to,
                kind: Some(body["kind"].as_str().unwrap_or("invoice").into()),
                confirm: true,
            };
            let (source, message) = material(tx, guard, &input, transport.identity())?;
            // Readback includes unknown/pending native evidence: never turn it into a resend.
            if let Some(delivery) = store::get(tx, &message.message_id)? {
                let row = delivery.row(true);
                if row["state"] != "accepted" {
                    return Ok(result(
                        "SMTP outcome unknown or refused; inspect append-only delivery evidence before retry",
                    ));
                }
                let mut value = json!({"ok":true,"duplicate":true,"appliedRules":["DK-EMAIL-DELIVERY-001"],"errors":[]});
                for k in ["invoiceNumber", "kind", "recipient", "subject", "messageId"] {
                    value[k] = row[k].clone();
                }
                return Ok(value);
            }
            if let Some(row) = store::rows(tx)?
                .into_iter()
                .find(|r| r["messageId"] == message.message_id)
            {
                if row["state"] != "accepted" {
                    return Ok(result(
                        "SMTP outcome unknown; inspect append-only delivery evidence before retry",
                    ));
                }
                let mut value = json!({"ok":true,"duplicate":true,"appliedRules":["DK-EMAIL-DELIVERY-001"],"errors":[]});
                for k in ["invoiceNumber", "kind", "recipient", "subject", "messageId"] {
                    value[k] = row[k].clone();
                }
                return Ok(value);
            }
            if let Some(reason) = transport.refusal() {
                return Ok(result(&format!("SMTP transport failed: {reason}")));
            }
            // This source built-in path has no external I/O; append an honest dry-run receipt.
            tx.execute("INSERT INTO email_send_log(invoice_document_id,invoice_no,kind,recipient,sender,subject,message_id,body_sha256,smtp_host) VALUES(?,?,?,?,?,?,?,?,?)",rusqlite::params![id,source.invoice_number,source.kind,source.recipient,source.sender.from_address.clone(),source.subject,source.message_id,source.body_sha256,source.sender.host]).map_err(|_|store::refused())?;
            tx.execute("INSERT INTO audit_log(event_type,entity_type,entity_id,message,actor,created_at) VALUES('email_sent','document',?,?,?,?)",rusqlite::params![id.to_string(),json!({"messageId":message.message_id,"dryRun":true}).to_string(),actor,at(context.clock)?]).map_err(|_|store::refused())?;
            Ok(
                json!({"ok":true,"invoiceNumber":source.invoice_number,"kind":source.kind,"recipient":source.recipient,"subject":source.subject,"messageId":source.message_id,"duplicate":false,"appliedRules":["DK-EMAIL-DELIVERY-001"],"errors":[]}),
            )
        },
    )
}
