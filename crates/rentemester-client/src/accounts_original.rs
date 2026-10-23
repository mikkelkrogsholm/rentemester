//! Source chart projection; all authority and accounting policy remain server-owned.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountsOriginalOperation {
    List,
    Add,
    RolesStatus,
    RoleConfirm,
}
use AccountsOriginalOperation::*;
const ROLES: &[&str] = &[
    "bank",
    "debtors",
    "creditors",
    "output_vat",
    "input_vat",
    "reverse_charge_vat",
    "vat_settlement",
    "operational_default",
];
const TYPES: &[&str] = &["asset", "liability", "equity", "income", "expense", "vat"];
fn text(max: usize) -> Value {
    json!({"type":"string","minLength":1,"maxLength":max})
}
pub fn original_accounts_schema(op: AccountsOriginalOperation) -> Value {
    match op {
        List | RolesStatus => crate::domain::object(json!({}), &[]),
        Add => crate::domain::object(
            json!({"confirm":{"const":true},"input":crate::domain::object(json!({"accountNo":text(64),"name":text(256),"type":{"type":"string","enum":TYPES},"normalBalance":{"type":["string","null"]},"defaultVatCode":{"type":["string","null"]},"allowDirectPosting":{"type":"boolean"}}), &["accountNo","name","type"])}),
            &["input", "confirm"],
        ),
        RoleConfirm => crate::domain::object(
            json!({"confirm":{"const":true},"role":{"type":"string","enum":ROLES},"accountNo":text(64)}),
            &["role", "accountNo", "confirm"],
        ),
    }
}
fn clean(v: &Value, max: usize) -> bool {
    v.as_str().is_some_and(|s| {
        !s.trim().is_empty() && s.trim().chars().count() <= max && !s.chars().any(char::is_control)
    })
}
fn identity(s: &str) -> bool {
    s.split_once(':').is_some_and(|(k, v)| {
        matches!(k, "user" | "service_account")
            && !v.is_empty()
            && !v.chars().any(|c| c.is_whitespace() || c.is_control())
    })
}
pub fn validate_original_accounts(
    op: AccountsOriginalOperation,
    input: &Value,
) -> Result<(), Error> {
    let bytes =
        serde_json::to_vec(input).map_err(|_| Error::InvalidInput("invalid accounts input"))?;
    if bytes.len() > 65536 {
        return Err(Error::InvalidInput("accounts input exceeds 65536 bytes"));
    }
    // The source trims chart fields before checking their shape. Validate the
    // normalized view while retaining the original wire payload and byte bound.
    let mut normalized = input.clone();
    if op == Add {
        for key in ["accountNo", "name", "type"] {
            if let Some(s) = input["input"][key].as_str() {
                normalized["input"][key] = json!(s.trim());
            }
        }
    }
    let normalized_bytes = serde_json::to_vec(&normalized)
        .map_err(|_| Error::InvalidInput("invalid accounts input"))?;
    crate::domain::body(&normalized_bytes, &original_accounts_schema(op))?;
    let v = if op == Add { &input["input"] } else { input };
    if matches!(op, Add | RoleConfirm)
        && (!clean(&v["accountNo"], 64) || op == Add && !clean(&v["name"], 256))
    {
        return Err(Error::InvalidInput(
            "nonempty bounded account number/name without controls required",
        ));
    }
    if op == Add
        && v.get("normalBalance").is_some_and(|v| {
            !v.is_null()
                && v.as_str()
                    .is_none_or(|s| !matches!(s.trim(), "" | "debit" | "credit"))
        })
    {
        return Err(Error::InvalidInput(
            "normal balance must be debit or credit",
        ));
    }
    Ok(())
}
fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == keys.len() && keys.iter().all(|k| o.contains_key(*k)))
}
fn real(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.trim().is_empty() && !s.chars().any(char::is_control))
}
fn role(v: &Value) -> bool {
    v.as_str().is_some_and(|s| ROLES.contains(&s))
}
// Historical provenance is evidence, not a current credential assertion.
fn historical_actor(actor: &Value, source: &Value) -> bool {
    actor.as_str().is_some_and(|s| {
        (s == "system" && source == "native_seed")
            || s.split_once(':').is_some_and(|(kind, subject)| {
                matches!(kind, "user" | "service_account" | "agent" | "system")
                    && !subject.is_empty()
                    && !subject.chars().any(|c| c.is_whitespace() || c.is_control())
            })
    })
}
fn resolution(v: &Value) -> bool {
    role(&v["role"])
        && if v["ok"] == true {
            exact(
                v,
                &[
                    "ok",
                    "role",
                    "accountNo",
                    "version",
                    "confirmedBy",
                    "confirmationSource",
                ],
            ) && clean(&v["accountNo"], 64)
                && v["version"]
                    .as_u64()
                    .is_some_and(|n| n > 0 && n <= 9007199254740991)
                && historical_actor(&v["confirmedBy"], &v["confirmationSource"])
                && real(&v["confirmationSource"])
        } else {
            v["ok"] == false && exact(v, &["ok", "role", "error"]) && real(&v["error"])
        }
}
fn status_valid(v: &Value) -> bool {
    let Some(rs) = v["roles"].as_array() else {
        return false;
    };
    if rs.len() != 8
        || !rs
            .iter()
            .zip(ROLES)
            .all(|(r, k)| r["role"] == *k && resolution(r))
    {
        return false;
    }
    let mut missing = vec![];
    let mut ambiguous = vec![];
    let Some(ps) = v["proposals"].as_array() else {
        return false;
    };
    if v["candidates"] != v["proposals"] {
        return false;
    }
    let mut last = None;
    for p in ps {
        if !role(&p["role"])
            || !clean(&p["accountNo"], 64)
            || !real(&p["source"])
            || !real(&p["proposedAt"])
            || !p["compatible"].is_boolean()
            || !if p["compatible"] == true {
                exact(
                    p,
                    &["role", "accountNo", "source", "proposedAt", "compatible"],
                )
            } else {
                exact(
                    p,
                    &[
                        "role",
                        "accountNo",
                        "source",
                        "proposedAt",
                        "compatible",
                        "reason",
                    ],
                ) && real(&p["reason"])
            }
        {
            return false;
        }
        let key = (
            p["role"].as_str().unwrap(),
            p["accountNo"].as_str().unwrap(),
            p["source"].as_str().unwrap(),
        );
        if last.is_some_and(|l| l > key) {
            return false;
        }
        last = Some(key);
    }
    for r in rs {
        if r["ok"] == true {
            continue;
        }
        let k = r["role"].as_str().unwrap();
        let accounts: std::collections::BTreeSet<_> = ps
            .iter()
            .filter(|p| p["role"] == k && p["compatible"] == true)
            .map(|p| p["accountNo"].as_str().unwrap())
            .collect();
        if accounts.len() > 1 {
            ambiguous.push(k);
        } else {
            missing.push(k);
        }
    }
    v["missing"] == json!(missing)
        && v["ambiguous"] == json!(ambiguous)
        && v["status"]
            == if !ambiguous.is_empty() {
                "ambiguous"
            } else if !missing.is_empty() {
                "incomplete"
            } else {
                "complete"
            }
        && v["reasons"].as_array().is_some_and(|a| {
            let unresolved: std::collections::BTreeSet<_> = rs
                .iter()
                .filter(|r| r["ok"] == false)
                .map(|r| r["role"].as_str().unwrap())
                .collect();
            let reasons: std::collections::BTreeSet<_> =
                a.iter().filter_map(|r| r["role"].as_str()).collect();
            a.len() == unresolved.len()
                && reasons == unresolved
                && a.iter().all(|r| {
                    exact(r, &["role", "reason"]) && role(&r["role"]) && real(&r["reason"])
                })
        })
}
fn valid(v: &Value, op: AccountsOriginalOperation, input: &Value, success: bool) -> bool {
    if !success {
        let base = v["ok"] == false
            && v["errors"]
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(real));
        return base
            && (exact(v, &["ok", "errors"])
                || op == RoleConfirm
                    && exact(
                        v,
                        &[
                            "ok",
                            "errors",
                            "status",
                            "missing",
                            "ambiguous",
                            "proposals",
                            "candidates",
                            "reasons",
                            "roles",
                        ],
                    )
                    && status_valid(v));
    }
    if v["ok"] != true {
        return false;
    }
    match op {
        List => {
            exact(v, &["ok", "count", "rows"])
                && v["rows"].as_array().is_some_and(|a| {
                    v["count"].as_u64() == Some(a.len() as u64)
                        && a.iter().all(|r| {
                            exact(r, &["account_no", "name", "type", "default_vat_code"])
                                && clean(&r["account_no"], 64)
                                && clean(&r["name"], 256)
                                && r["type"].as_str().is_some_and(|s| TYPES.contains(&s))
                                && (r["default_vat_code"].is_null() || real(&r["default_vat_code"]))
                        })
                        && a.windows(2)
                            .all(|p| p[0]["account_no"].as_str() < p[1]["account_no"].as_str())
                })
        }
        Add => {
            exact(v, &["ok", "accountNo", "appliedRules", "errors"])
                && v["accountNo"] == input["input"]["accountNo"].as_str().unwrap().trim()
                && v["errors"] == json!([])
                && v["appliedRules"] == json!(["DK-CHART-OF-ACCOUNTS-ADD-001"])
        }
        RolesStatus => {
            exact(
                v,
                &[
                    "ok",
                    "status",
                    "missing",
                    "ambiguous",
                    "proposals",
                    "candidates",
                    "reasons",
                    "roles",
                ],
            ) && status_valid(v)
        }
        RoleConfirm => {
            exact(
                v,
                &[
                    "ok",
                    "resolution",
                    "status",
                    "missing",
                    "ambiguous",
                    "proposals",
                    "candidates",
                    "reasons",
                    "roles",
                ],
            ) && status_valid(v)
                && resolution(&v["resolution"])
                && v["resolution"]["ok"] == true
                && v["resolution"]["role"] == input["role"]
                && v["resolution"]["accountNo"] == input["accountNo"].as_str().unwrap().trim()
                && v["roles"].as_array().unwrap().contains(&v["resolution"])
        }
    }
}
impl ApiClient {
    /// One request only. Unknown writes must be reconciled by canonical reads, never retried.
    pub async fn original_accounts(
        &self,
        company: &str,
        op: AccountsOriginalOperation,
        input: &Value,
        actor: Option<&str>,
        principal: Option<&str>,
    ) -> Result<Value, Error> {
        validate_original_accounts(op, input)?;
        let write = matches!(op, Add | RoleConfirm);
        if (actor.is_some() || principal.is_some()) && !write
            || actor.into_iter().chain(principal).any(|s| !identity(s))
        {
            return Err(Error::InvalidInput(
                "typed credential assertions are write-only",
            ));
        }
        let mut body = input.clone();
        for (k, v) in [("actor", actor), ("principal", principal)] {
            if let Some(v) = v {
                body[k] = json!(v);
            }
        }
        let bytes =
            serde_json::to_vec(&body).map_err(|_| Error::InvalidInput("invalid accounts body"))?;
        if bytes.len() > 65536 {
            return Err(Error::InvalidInput(
                "accounts wire body exceeds 65536 bytes",
            ));
        }
        let suffix = match op {
            List => "list",
            Add => "add",
            RolesStatus => "roles-status",
            RoleConfirm => "role-confirm",
        };
        let uncertain = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut response = self
            .send(
                if write { Method::POST } else { Method::GET },
                &company_path(company, &format!("original/accounts/{suffix}"))?,
                &[],
                if write { Some(bytes) } else { None },
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if !write && e == Error::OutcomeUnknown {
                    Error::Transport
                } else {
                    e
                }
            })?;
        let status = response.status().as_u16();
        if status >= 500 || status == 408 {
            return Err(if write {
                Error::OutcomeUnknown
            } else {
                Error::Transport
            });
        }
        if status != 200 && status != 422 {
            return Err(Error::Refused { status });
        }
        let mut bytes = vec![];
        while let Some(chunk) = response.chunk().await.map_err(|_| uncertain())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(uncertain());
            }
            bytes.extend_from_slice(&chunk);
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| uncertain())?;
        if valid(&v, op, input, status == 200) {
            Ok(v)
        } else {
            Err(uncertain())
        }
    }
}
