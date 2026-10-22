//! Additive source-flat master-data contracts, using the current authenticated transport.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MasterDataOriginalKind {
    Customer,
    Vendor,
}
impl MasterDataOriginalKind {
    fn path(self) -> &'static str {
        match self {
            Self::Customer => "customers",
            Self::Vendor => "vendors",
        }
    }
    fn rule(self) -> &'static str {
        match self {
            Self::Customer => "DK-MASTER-DATA-CUSTOMER-001",
            Self::Vendor => "DK-MASTER-DATA-VENDOR-001",
        }
    }
}
pub fn master_data_original_create_schema(kind: MasterDataOriginalKind) -> Value {
    let mut properties = json!({"name":{"type":"string","minLength":1}});
    let fields: &[&str] = match kind {
        MasterDataOriginalKind::Customer => &[
            "address",
            "vatOrCvr",
            "email",
            "phone",
            "website",
            "eanNumber",
            "defaultCurrency",
            "notes",
        ],
        MasterDataOriginalKind::Vendor => &[
            "address",
            "vatOrCvr",
            "email",
            "phone",
            "website",
            "defaultExpenseAccount",
            "defaultVatTreatment",
            "notes",
        ],
    };
    for field in fields {
        properties[*field] = json!({"type":"string"});
    }
    match kind {
        MasterDataOriginalKind::Customer => {
            properties["paymentTermsDays"] = json!({"type":"integer","minimum":1})
        }
        MasterDataOriginalKind::Vendor => {
            properties["countryCode"] = json!({"type":"string","minLength":2,"maxLength":2});
            properties["identifierKind"] =
                json!({"type":"string","enum":["dk_cvr","eu_vat","non_eu"]});
        }
    }
    json!({"type":"object","additionalProperties":false,"properties":properties})
}
fn keys(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.keys().all(|k| fields.contains(&k.as_str())))
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    keys(v, fields) && v.as_object().is_some_and(|o| o.len() == fields.len())
}
fn text(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
fn nullable(v: &Value) -> bool {
    v.is_null() || v.is_string()
}
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(text))
}
fn id(v: &Value) -> bool {
    v.as_u64()
        .is_some_and(|n| n > 0 && n <= 9_007_199_254_740_991)
}
fn row(v: &Value, kind: MasterDataOriginalKind) -> bool {
    let fields: &[&str] = match kind {
        MasterDataOriginalKind::Customer => &[
            "id",
            "name",
            "address",
            "vatOrCvr",
            "email",
            "phone",
            "website",
            "eanNumber",
            "paymentTermsDays",
            "defaultCurrency",
            "notes",
            "archived",
            "createdAt",
        ],
        MasterDataOriginalKind::Vendor => &[
            "id",
            "name",
            "address",
            "vatOrCvr",
            "countryCode",
            "identifierKind",
            "identityStatus",
            "email",
            "phone",
            "website",
            "defaultExpenseAccount",
            "defaultVatTreatment",
            "notes",
            "archived",
            "createdAt",
        ],
    };
    exact(v, fields)
        && id(&v["id"])
        && text(&v["name"])
        && text(&v["createdAt"])
        && v["archived"].is_boolean()
        && fields
            .iter()
            .filter(|k| {
                !matches!(
                    **k,
                    "id" | "name" | "createdAt" | "archived" | "paymentTermsDays"
                )
            })
            .all(|k| nullable(&v[*k]))
        && (kind != MasterDataOriginalKind::Customer
            || v["paymentTermsDays"].is_null()
            || id(&v["paymentTermsDays"]))
}
#[derive(Clone, Copy)]
enum Operation {
    List(MasterDataOriginalKind, bool),
    Create(MasterDataOriginalKind),
    Vat,
    Cvr,
}
fn valid(v: &Value, op: Operation, strict: bool) -> bool {
    if !v["ok"].is_boolean()
        || !strings(&v["errors"])
        || v.get("code").is_some_and(|v| !text(v))
        || v.get("appliedRules").is_some_and(|v| !strings(v))
    {
        return false;
    }
    if v["ok"] == false {
        let allowed: &[&str] = if matches!(op, Operation::Cvr) {
            &[
                "ok",
                "errors",
                "code",
                "status",
                "appliedRules",
                "cached",
                "fetchedAt",
                "company",
            ]
        } else {
            &["ok", "errors", "code", "status", "appliedRules"]
        };
        return keys(v, allowed)
            && !v["errors"].as_array().unwrap().is_empty()
            && (!strict || text(&v["code"]))
            && v.get("status").is_none_or(text)
            && v.get("cached").is_none_or(Value::is_boolean)
            && v.get("fetchedAt").is_none_or(text)
            && v.get("company").is_none_or(Value::is_null);
    }
    if strict || !v["errors"].as_array().unwrap().is_empty() {
        return false;
    }
    match op {
        Operation::List(kind, archived) => {
            exact(v, &["ok", "count", "rows", "errors"])
                && v["rows"].as_array().is_some_and(|rows| {
                    v["count"].as_u64() == Some(rows.len() as u64)
                        && rows
                            .iter()
                            .all(|r| row(r, kind) && (archived || r["archived"] == false))
                        && rows.windows(2).all(|p| {
                            let a = p[0]["name"].as_str().unwrap().to_ascii_lowercase();
                            let b = p[1]["name"].as_str().unwrap().to_ascii_lowercase();
                            a < b || (a == b && p[0]["id"].as_u64() < p[1]["id"].as_u64())
                        })
                })
        }
        Operation::Create(kind) => {
            let field = if kind == MasterDataOriginalKind::Customer {
                "customerId"
            } else {
                "vendorId"
            };
            exact(v, &["ok", field, "appliedRules", "errors"])
                && id(&v[field])
                && v["appliedRules"] == json!([kind.rule()])
        }
        Operation::Vat => {
            exact(v, &["ok", "validation", "appliedRules", "errors"])
                && v["appliedRules"] == json!(["DK-VAT-REVERSE-CHARGE-001"])
                && exact(
                    &v["validation"],
                    &[
                        "countryCode",
                        "vatNumber",
                        "normalized",
                        "valid",
                        "name",
                        "address",
                        "validatedAt",
                        "expiresAt",
                        "rawResponse",
                    ],
                )
                && [
                    "countryCode",
                    "vatNumber",
                    "normalized",
                    "validatedAt",
                    "expiresAt",
                ]
                .iter()
                .all(|k| text(&v["validation"][*k]))
                && v["validation"]["valid"].is_boolean()
                && ["name", "address", "rawResponse"]
                    .iter()
                    .all(|k| nullable(&v["validation"][*k]))
        }
        Operation::Cvr => {
            keys(v, &["ok", "company", "cached", "fetchedAt", "errors"])
                && v["cached"].is_boolean()
                && v.get("fetchedAt").is_none_or(text)
                && cvr_company(&v["company"])
        }
    }
}
fn cvr_company(v: &Value) -> bool {
    let texts = [
        "address",
        "postalCode",
        "city",
        "companyFormShort",
        "companyFormLong",
        "status",
        "industryCode",
        "industryText",
        "email",
        "phone",
        "website",
        "startDate",
        "fiscalYearStart",
        "fiscalYearEnd",
        "shareCapitalCurrency",
    ];
    let numbers = [
        "municipalityCode",
        "companyFormCode",
        "fiscalYearStartMonth",
        "shareCapital",
        "employees",
    ];
    let mut fields = vec![
        "cvr",
        "name",
        "auditWaived",
        "advertisingProtected",
        "management",
    ];
    fields.extend(texts);
    fields.extend(numbers);
    exact(v, &fields)
        && text(&v["cvr"])
        && text(&v["name"])
        && texts.iter().all(|k| nullable(&v[*k]))
        && numbers.iter().all(|k| v[*k].is_null() || v[*k].is_number())
        && (v["auditWaived"].is_null() || v["auditWaived"].is_boolean())
        && v["advertisingProtected"].is_boolean()
        && v["management"].as_array().is_some_and(|a| {
            a.iter().all(|m| {
                exact(m, &["name", "role"]) && m["name"].is_string() && m["role"].is_string()
            })
        })
}
impl ApiClient {
    pub async fn master_data_original_list(
        &self,
        company: &str,
        kind: MasterDataOriginalKind,
        archived: bool,
    ) -> Result<Value, Error> {
        self.master_original_request(
            company,
            kind.path(),
            &[("archived", if archived { "true" } else { "false" })],
            None,
            Operation::List(kind, archived),
        )
        .await
    }
    pub async fn master_data_original_create(
        &self,
        company: &str,
        kind: MasterDataOriginalKind,
        bytes: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed || bytes.len() > 256 * 1024 {
            return Err(Error::InvalidInput(
                "confirmed bounded contact body required",
            ));
        }
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|_| Error::InvalidInput("invalid contact body"))?;
        if !keys(&v, &["input", "fromCvr", "confirm"])
            || v["confirm"] != true
            || v.get("fromCvr").is_some_and(|v| !v.is_string())
        {
            return Err(Error::InvalidInput(
                "contact requires input, optional fromCvr and confirm true",
            ));
        }
        crate::domain::body(
            &serde_json::to_vec(&v["input"]).unwrap(),
            &master_data_original_create_schema(kind),
        )?;
        self.master_original_request(
            company,
            kind.path(),
            &[],
            Some(bytes),
            Operation::Create(kind),
        )
        .await
    }
    pub async fn customer_original_validate_vat(
        &self,
        company: &str,
        cvr: &str,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if !confirmed {
            return Err(Error::InvalidInput("VAT validation confirmation required"));
        }
        let bytes = serde_json::to_vec(&json!({"cvr":cvr,"confirm":true})).unwrap();
        if bytes.len() > 256 * 1024 {
            return Err(Error::InvalidInput("VAT body exceeds256KiB"));
        }
        self.master_original_request(
            company,
            "customer-validate-vat",
            &[],
            Some(bytes),
            Operation::Vat,
        )
        .await
    }
    pub async fn master_data_original_cvr_lookup(
        &self,
        company: &str,
        cvr: &str,
    ) -> Result<Value, Error> {
        if cvr.len() > 128 {
            return Err(Error::InvalidInput("CVR lookup text exceeds128 bytes"));
        }
        self.master_original_request(company, "cvr-lookup", &[("cvr", cvr)], None, Operation::Cvr)
            .await
    }
    async fn master_original_request(
        &self,
        company: &str,
        suffix: &str,
        query: &[(&str, &str)],
        body: Option<Vec<u8>>,
        op: Operation,
    ) -> Result<Value, Error> {
        let effect = !matches!(op, Operation::List(..));
        let path = company_path(company, &format!("master-data/original/{suffix}"))?;
        let mut response = self
            .send(
                if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                &path,
                query,
                body,
                self.cookie.as_ref().map(|c| c.as_str()),
            )
            .await
            .map_err(|e| {
                if effect && e == Error::Transport {
                    Error::OutcomeUnknown
                } else {
                    e
                }
            })?;
        let status = response.status();
        let strict = status.as_u16() == 422;
        if effect && (status.is_server_error() || status.as_u16() == 408) {
            return Err(Error::OutcomeUnknown);
        }
        if !status.is_success() && !strict {
            return Err(Error::Refused {
                status: status.as_u16(),
            });
        }
        let protocol = || {
            if strict {
                Error::Refused { status: 422 }
            } else if effect {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| protocol())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(protocol());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value = serde_json::from_slice(&bytes).map_err(|_| protocol())?;
        if valid(&value, op, strict) {
            Ok(value)
        } else {
            Err(protocol())
        }
    }
}
