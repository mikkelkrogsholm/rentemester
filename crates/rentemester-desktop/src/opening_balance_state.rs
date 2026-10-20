//! Reviewed server opening plans. No accounting calculations, inferred cutovers or source amounts.
use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub profile: Value,
    pub chart: Value,
    pub me: Value,
    pub journal: Value,
    pub opening: Value,
    pub principal: String,
    pub ledger: String,
}
fn error() -> String {
    "Serverens primobalancegrundlag kunne ikke verificeres; intet sendt".into()
}
fn required(value: &Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && !s.chars().any(char::is_control))
        .map(str::to_owned)
        .ok_or_else(error)
}
fn hex(value: &Value, n: usize) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == n
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
/// Syntax only: balances and all domain decisions belong to the server.
fn money(s: &str) -> bool {
    let Some((whole, fraction)) = s.split_once('.') else {
        return false;
    };
    !whole.is_empty()
        && (whole == "0" || !whole.starts_with('0'))
        && whole.bytes().all(|b| b.is_ascii_digit())
        && fraction.len() == 2
        && fraction.bytes().all(|b| b.is_ascii_digit())
}
fn date(s: &str) -> bool {
    s.len() == 10
        && s.bytes().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}
impl Snapshot {
    pub fn parse(
        company: &str,
        profile: Value,
        chart: Value,
        me: Value,
        journal: Value,
        opening: Value,
    ) -> Result<Self, String> {
        if [&profile, &chart, &me, &journal, &opening]
            .iter()
            .any(|v| v["ok"] != true)
            || chart["accounts"]["slug"] != company
            || journal["journal"]["slug"] != company
            || !profile["company"].is_object()
            || !chart["accounts"]["accounts"].is_array()
            || chart["accounts"]["company"] != profile["company"]
            || !journal["journal"]["entries"].is_array()
            || !hex(&journal["journal"]["ledgerUuid"], 32)
            || opening.get("openingBalance").is_none()
        {
            return Err(error());
        }
        let principal = required(&me["user"], "id")?;
        let ledger = required(&journal["journal"], "ledgerUuid")?;
        Ok(Self {
            profile: profile["company"].clone(),
            chart: chart["accounts"].clone(),
            me,
            journal: journal["journal"].clone(),
            opening: opening["openingBalance"].clone(),
            principal,
            ledger,
        })
    }
}
#[derive(Clone, Debug, Default)]
pub struct LineInput {
    pub account_no: String,
    pub debit: String,
    pub credit: String,
    pub text: String,
}
pub fn input(
    cut_over_date: &str,
    lines: &[LineInput],
    note: &str,
    citations: &[i64],
    snapshot: &Snapshot,
) -> Result<Vec<u8>, String> {
    if !date(cut_over_date)
        || lines.len() < 2
        || note.len() > 4096
        || citations.len() > 32
        || citations
            .iter()
            .any(|n| !(1..=9_007_199_254_740_991).contains(n))
        || citations
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != citations.len()
    {
        return Err("Udfyld eksplicit dato, mindst to linjer og gyldige arkivreferencer".into());
    }
    let accounts = snapshot.chart["accounts"].as_array().ok_or_else(error)?;
    let mut output = Vec::new();
    for line in lines {
        if !accounts
            .iter()
            .any(|a| a["accountNo"] == line.account_no && a["active"] == true)
        {
            return Err("Vælg en aktiv konto fra serverens kontoplan".into());
        }
        let debit = if line.debit.is_empty() {
            "0.00"
        } else {
            &line.debit
        };
        let credit = if line.credit.is_empty() {
            "0.00"
        } else {
            &line.credit
        };
        if !money(debit) || !money(credit) || (debit == "0.00") == (credit == "0.00") {
            return Err(
                "Angiv ét positivt debet- eller kreditbeløb som fx 100.00 på hver linje".into(),
            );
        }
        output.push(json!({"accountNo":line.account_no,"debitAmount":debit,"creditAmount":credit,"text":if line.text.is_empty(){Value::Null}else{json!(line.text)}}));
    }
    let value = json!({"cutOverDate":cut_over_date,"lines":output,"note":if note.is_empty(){Value::Null}else{json!(note)},"sourceArchiveImportIds":citations});
    let bytes = serde_json::to_vec(&value).map_err(|_| error())?;
    if bytes.len() > 65536 {
        return Err("Primobalancen overskrider anmodningsgrænsen".into());
    }
    Ok(bytes)
}
fn input_shape(v: &Value) -> bool {
    let Some(o) = v.as_object() else {
        return false;
    };
    o.len() == 4
        && ["cutOverDate", "lines", "note", "sourceArchiveImportIds"]
            .iter()
            .all(|k| o.contains_key(*k))
        && v["cutOverDate"].as_str().is_some_and(date)
        && v["lines"].as_array().is_some_and(|rows| {
            rows.len() >= 2
                && rows.iter().all(|row| {
                    let Some(r) = row.as_object() else {
                        return false;
                    };
                    r.len() == 4
                        && ["accountNo", "debitAmount", "creditAmount", "text"]
                            .iter()
                            .all(|k| r.contains_key(*k))
                        && row["accountNo"].as_str().is_some_and(|s| !s.is_empty())
                        && ["debitAmount", "creditAmount"]
                            .iter()
                            .all(|k| row[*k].as_str().is_some_and(money))
                        && (row["text"].is_null() || row["text"].is_string())
                })
        })
        && (v["note"].is_null() || v["note"].as_str().is_some_and(|s| s.len() <= 4096))
        && v["sourceArchiveImportIds"].as_array().is_some_and(|ids| {
            ids.len() <= 32
                && ids.iter().all(|id| {
                    id.as_i64()
                        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
                })
                && ids
                    .iter()
                    .map(Value::to_string)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == ids.len()
        })
}
fn plan_shape(input: &Value, plan: &Value) -> bool {
    let Some(ids) = input["sourceArchiveImportIds"].as_array() else {
        return false;
    };
    input_shape(input)
        && plan["input"] == *input
        && hex(&plan["planHash"], 64)
        && plan["ruleVersion"].as_str().is_some_and(|s| !s.is_empty())
        && matches!(plan["status"].as_str(), Some("ready" | "blocked"))
        && plan["blockers"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(Value::is_string))
        && ["debit", "credit"]
            .iter()
            .all(|k| plan["totals"][*k].as_str().is_some_and(money))
        && plan["sourceBasis"]["kind"] == "operator_supplied"
        && plan["sourceBasis"]["currency"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && plan["sourceBasis"]["sourceArchives"]
            .as_array()
            .is_some_and(|rows| {
                rows.len() == ids.len()
                    && rows.iter().zip(ids).all(|(r, id)| {
                        r["importId"] == *id
                            && hex(&r["sourceHash"], 64)
                            && r["parserContract"].as_str().is_some_and(|s| !s.is_empty())
                            && r["declaredCurrency"]
                                .as_str()
                                .is_some_and(|s| !s.is_empty())
                    })
            })
        && plan.get("existing").is_some()
}
#[derive(Clone, Debug)]
pub struct Review {
    pub before: Snapshot,
    pub input: Value,
    pub plan: Value,
}
impl Review {
    pub fn parse(before: Snapshot, input: &[u8], response: Value) -> Result<Self, String> {
        let input: Value = serde_json::from_slice(input).map_err(|_| error())?;
        if response["ok"] != true
            || !plan_shape(&input, &response["plan"])
            || response["plan"]["sourceBasis"]["currency"] != before.profile["currency"]
        {
            return Err(error());
        }
        Ok(Self {
            before,
            input,
            plan: response["plan"].clone(),
        })
    }
    pub fn current(&self, now: &Snapshot) -> bool {
        self.before == *now
    }
    pub fn ready(&self) -> bool {
        plan_shape(&self.input, &self.plan)
            && self.plan["status"] == "ready"
            && self.plan["existing"].is_null()
            && self.plan["blockers"].as_array().is_some_and(Vec::is_empty)
    }
    pub fn apply_body(&self, key: &str) -> Result<Vec<u8>, String> {
        if !self.ready() || key.trim().is_empty() || key.encode_utf16().count() > 128 {
            return Err("En klar gennemgået serverplan og den oprindelige nøgle kræves".into());
        }
        let bytes=serde_json::to_vec(&json!({"input":self.input,"planHash":self.plan["planHash"],"idempotencyKey":key,"confirm":true})).map_err(|_|error())?;
        if bytes.len() > 65536 {
            return Err("Primobalancen overskrider anmodningsgrænsen".into());
        }
        Ok(bytes)
    }
    pub fn receipt(&self, value: &Value) -> bool {
        self.canonical(value)
            && value["idempotency"]["receiptId"]
                .as_i64()
                .is_some_and(|id| id > 0)
            && value["idempotency"]["replayed"].is_boolean()
            && ["createdAt", "expiresAt"].iter().all(|k| {
                value["idempotency"][*k]
                    .as_str()
                    .is_some_and(|s| !s.is_empty())
            })
    }
    pub fn canonical(&self, value: &Value) -> bool {
        let r = &value["openingBalance"];
        value["ok"] == true
            && plan_shape(&self.input, &self.plan)
            && r["planHash"] == self.plan["planHash"]
            && r["cutOverDate"] == self.input["cutOverDate"]
            && r["sourceBasis"] == self.plan["sourceBasis"]
            && r["journalEntryId"].as_i64().is_some_and(|id| id > 0)
            && r["journalEntryNo"].as_str().is_some_and(|s| !s.is_empty())
            && hex(&r["journalEntryHash"], 64)
            && r["actor"] == format!("user:{}", self.before.principal)
            && r["createdAt"].as_str().is_some_and(|s| !s.is_empty())
    }
}
pub fn journal_matches(response: &Value, record: &Value, company: &str, ledger: &str) -> bool {
    response["ok"] == true
        && response["journal"]["slug"] == company
        && response["journal"]["ledgerUuid"] == ledger
        && response["journal"]["entries"]
            .as_array()
            .is_some_and(|rows| {
                rows.iter().any(|row| {
                    row["id"] == record["journalEntryId"]
                        && row["entryNo"] == record["journalEntryNo"]
                        && row["date"] == record["cutOverDate"]
                        && row["reversalOfEntryId"].is_null()
                        && row["reversedByEntryId"].is_null()
                })
            })
}
/// Only server-advertised live bounds; never derive a fiscal label from a calendar year.
pub fn journal_year(response: &Value, cutover: &str) -> Option<String> {
    if response["ok"] != true || !date(cutover) {
        return None;
    }
    let rows = response["journal"]["fiscalYears"].as_array()?;
    let selected = rows
        .iter()
        .filter(|r| {
            r["source"] == "live"
                && r["start"].as_str().is_some_and(|s| s <= cutover)
                && r["end"].as_str().is_some_and(|s| s >= cutover)
        })
        .collect::<Vec<_>>();
    if selected.len() != 1 {
        return None;
    }
    selected[0]["label"].as_str().map(str::to_owned)
}
/// Durable attempts may replay only the original exact reviewed body, never a reconstructed plan.
pub(crate) fn attempt_body(bytes: &[u8], plan: &Value) -> Result<(), String> {
    if bytes.len() > 65536 {
        return Err(error());
    }
    let body: Value = serde_json::from_slice(bytes).map_err(|_| error())?;
    let Some(o) = body.as_object() else {
        return Err(error());
    };
    if o.len() != 4
        || !["input", "planHash", "idempotencyKey", "confirm"]
            .iter()
            .all(|k| o.contains_key(*k))
        || body["confirm"] != true
        || !plan_shape(&body["input"], plan)
        || body["planHash"] != plan["planHash"]
        || body["idempotencyKey"]
            .as_str()
            .is_none_or(|k| k.trim().is_empty() || k.encode_utf16().count() > 128)
    {
        return Err(error());
    }
    Ok(())
}
#[cfg(test)]
pub(crate) mod tests;
