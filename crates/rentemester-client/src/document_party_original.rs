//! Original party decisions. Attribution assertions never grant authority; the server
//! compares them to its current admitted actor/principal before receipt lookup.
//! Established admission projections are `user:{subject}` for humans and
//! `service_account:{subject}` for typed service accounts, never email/key strings.
use crate::{ApiClient, Error, MAX_RESPONSE, company_path};
use reqwest::Method;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentPartyOriginalOperation {
    Plan,
    Apply,
    Supersede,
    Internal,
    InternalSupersede,
}
use DocumentPartyOriginalOperation::*;
const ROLES: &[&str] = &[
    "issuer",
    "supplier",
    "customer",
    "recipient",
    "payer",
    "payee",
    "processor",
    "acquirer",
    "related_company",
    "establishment",
    "location",
    "payment_descriptor",
    "vendor",
    "owner",
    "adviser",
    "employee",
    "authority",
    "bank",
];
fn text(min: usize, max: usize) -> Value {
    json!({"type":"string","minLength":min,"maxLength":max})
}
fn hash_schema() -> Value {
    json!({"type":"string","minLength":64,"maxLength":64,"pattern":"^[0-9a-fA-F]{64}$"})
}
pub fn document_party_original_schema(op: DocumentPartyOriginalOperation) -> Value {
    let mut p = json!({"documentId":{"type":"integer","minimum":1,"maximum":9007199254740991u64}});
    let mut required = vec!["documentId"];
    if matches!(op, Plan | Apply | Supersede) {
        p["role"] = json!({"type":"string","enum":ROLES});
        required.push("role");
    }
    if matches!(op, Plan | Apply) {
        for (k, v) in [
            ("partyId", text(3, 64)),
            ("jurisdiction", text(2, 2)),
            (
                "identifierKind",
                json!({"type":"string","enum":["dk_cvr","eu_vat","non_eu"]}),
            ),
            ("identifier", text(1, 160)),
            (
                "legacyKind",
                json!({"type":"string","enum":["customer","vendor"]}),
            ),
            ("legacyId", text(1, 160)),
            ("reviewedLegacyReference", text(1, 500)),
        ] {
            p[k] = v;
        }
    }
    if matches!(op, Apply | Supersede) {
        p["planHash"] = hash_schema();
        required.push("planHash");
    }
    if op == InternalSupersede {
        p["decisionHash"] = hash_schema();
        required.push("decisionHash");
    }
    if matches!(op, Supersede | Internal | InternalSupersede) {
        p["reason"] = text(1, 1000);
        required.push("reason");
    }
    if matches!(op, Apply | Internal) {
        p["idempotencyKey"] = text(1, 128);
    }
    if op != Plan {
        p["confirm"] = json!({"const":true});
        required.push("confirm");
        p["expectedActor"] = text(1, 160);
        p["expectedPrincipal"] = text(1, 160);
    }
    crate::domain::object(p, &required)
}
fn hash(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
}
fn id(v: &Value) -> bool {
    v.as_i64()
        .is_some_and(|n| (1..=9_007_199_254_740_991).contains(&n))
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
}
fn nonblank(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.trim().is_empty())
}
/// Also enforces syntax/nonblank constraints not supported by the common shape validator.
pub fn document_party_original_validate(
    op: DocumentPartyOriginalOperation,
    bytes: &[u8],
) -> Result<(), Error> {
    if bytes.len() > 16384 {
        return Err(Error::InvalidInput("party decision exceeds 16KiB"));
    }
    crate::domain::body(bytes, &document_party_original_schema(op))?;
    let v: Value =
        serde_json::from_slice(bytes).map_err(|_| Error::InvalidInput("invalid JSON"))?;
    for k in ["planHash", "decisionHash"] {
        if v.get(k).is_some_and(|v| !hash(v)) {
            return Err(Error::InvalidInput("64 hex hash required"));
        }
    }
    for k in [
        "reason",
        "reviewedLegacyReference",
        "expectedActor",
        "expectedPrincipal",
    ] {
        if v.get(k).is_some_and(|v| !nonblank(v)) {
            return Err(Error::InvalidInput("nonblank decision text required"));
        }
    }
    Ok(())
}
const DOMAIN: &[&str] = &[
    "DOCUMENT_NOT_FOUND",
    "PARTY_NOT_FOUND",
    "SCOPE_MISMATCH",
    "NO_IDENTIFIER_EVIDENCE",
    "IDENTIFIER_CONFLICT",
    "MULTIPLE_CANDIDATES",
    "LEGACY_REFERENCE_UNREVIEWED",
    "PLAN_HASH_MISMATCH",
    "CONFIRMATION_REQUIRED",
    "ACTOR_REQUIRED",
    "PRINCIPAL_REQUIRED",
    "LINK_NOT_FOUND",
    "SUPERSEDE_REASON_REQUIRED",
    "IDEMPOTENCY_CONFLICT",
    "CURRENT_STATE_CONFLICT",
];
fn strings(v: &Value) -> bool {
    v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
}
fn failure(v: &Value) -> bool {
    v["ok"] == false
        && v.as_object().is_some_and(|o| {
            o.keys()
                .all(|k| ["ok", "errors", "code", "appliedRules"].contains(&k.as_str()))
        })
        && strings(&v["errors"])
        && v["errors"].as_array().is_some_and(|a| !a.is_empty())
        && v.get("code").is_none_or(nonblank)
        && v.get("appliedRules").is_none_or(strings)
}
fn curated(v: &Value) -> bool {
    failure(v)
        && (v
            .get("code")
            .is_some_and(|s| s.as_str().is_some_and(|s| DOMAIN.contains(&s)))
            || (v.get("code").is_none()
                && v["errors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|s| DOMAIN.contains(&s.as_str().unwrap()))))
}

fn evidence(v: &Value) -> bool {
    match v["kind"].as_str() {
        Some("exact_identifier") => {
            exact(v, &["kind", "jurisdiction", "identifierKind", "identifier"])
                && v["jurisdiction"].as_str().is_some_and(|s| s.len() == 2)
                && ["dk_cvr", "eu_vat", "non_eu"]
                    .contains(&v["identifierKind"].as_str().unwrap_or(""))
                && nonblank(&v["identifier"])
        }
        Some("reviewed_legacy_reference") => {
            exact(
                v,
                &["kind", "legacyKind", "legacyId", "reviewedLegacyReference"],
            ) && ["customer", "vendor"].contains(&v["legacyKind"].as_str().unwrap_or(""))
                && nonblank(&v["legacyId"])
                && nonblank(&v["reviewedLegacyReference"])
        }
        _ => false,
    }
}
fn snapshot(v: &Value) -> bool {
    exact(
        v,
        &["partyId", "kind", "name", "identifiers", "roles", "history"],
    ) && nonblank(&v["partyId"])
        && nonblank(&v["name"])
        && [
            "person",
            "organization",
            "public_authority",
            "financial_institution",
        ]
        .contains(&v["kind"].as_str().unwrap_or(""))
        && v["identifiers"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                r.as_object().is_some_and(|o| {
                    o.keys()
                        .all(|k| ["country", "identifierKind", "identifier"].contains(&k.as_str()))
                }) && r["country"].is_string()
                    && r["identifierKind"].is_string()
                    && (r["identifier"].is_string() || r["identifier"].is_null())
            })
        })
        && v["roles"].as_array().is_some_and(|a| {
            !a.is_empty()
                && a.iter().all(|r| {
                    exact(r, &["companySlug", "role", "defaults"])
                        && nonblank(&r["companySlug"])
                        && ROLES.contains(&r["role"].as_str().unwrap_or(""))
                        && r["defaults"].as_object().is_some_and(|o| {
                            o.iter().all(|(k, v)| {
                                ["account", "vat", "currency", "paymentTermsDays"]
                                    .contains(&k.as_str())
                                    && (v.is_string()
                                        || v.is_number()
                                        || v.is_boolean()
                                        || v.is_null())
                            })
                        })
                })
        })
        && v["history"].as_array().is_some_and(|a| {
            a.iter().all(|r| {
                exact(r, &["event_type", "payload_hash", "actor", "created_at"])
                    && nonblank(&r["event_type"])
                    && hash(&r["payload_hash"])
                    && nonblank(&r["actor"])
                    && nonblank(&r["created_at"])
            })
        })
}
fn valid(v: &Value, op: Option<DocumentPartyOriginalOperation>, inspect: bool) -> bool {
    if v["ok"] == false {
        return failure(v);
    }
    if v["ok"] != true {
        return false;
    }
    match op {
        Some(Plan) => {
            let p = &v["plan"];
            exact(v, &["ok", "plan"])
                && exact(
                    p,
                    &[
                        "documentId",
                        "documentSha256",
                        "documentPayloadSha256",
                        "partyId",
                        "partySnapshot",
                        "role",
                        "evidence",
                        "planHash",
                    ],
                )
                && id(&p["documentId"])
                && ["documentSha256", "documentPayloadSha256", "planHash"]
                    .iter()
                    .all(|k| hash(&p[*k]))
                && nonblank(&p["partyId"])
                && ROLES.contains(&p["role"].as_str().unwrap_or(""))
                && evidence(&p["evidence"])
                && snapshot(&p["partySnapshot"])
                && p["partyId"] == p["partySnapshot"]["partyId"]
        }
        Some(Apply | Internal) => {
            let k = if op == Some(Apply) {
                "planHash"
            } else {
                "decisionHash"
            };
            exact(v, &["ok", "id", "idempotent", k])
                && id(&v["id"])
                && v["idempotent"].is_boolean()
                && hash(&v[k])
        }
        Some(Supersede | InternalSupersede) => {
            exact(v, &["ok", "id", "idempotent"])
                && (v["id"].is_null() || id(&v["id"]))
                && v["idempotent"].is_boolean()
        }
        None => {
            exact(v, &["ok", "links"])
                && v["links"].as_array().is_some_and(|a| {
                    if inspect {
                        a.iter().all(|r| {
                            exact(
                                r,
                                &[
                                    "id",
                                    "party_id",
                                    "party_role",
                                    "event_type",
                                    "evidence_kind",
                                    "evidence_json",
                                    "document_sha256",
                                    "document_payload_sha256",
                                    "party_snapshot_json",
                                    "plan_hash",
                                    "reason",
                                    "actor",
                                    "principal",
                                    "created_at",
                                ],
                            ) && id(&r["id"])
                                && ROLES.contains(&r["party_role"].as_str().unwrap_or(""))
                                && ["linked", "superseded"]
                                    .contains(&r["event_type"].as_str().unwrap_or(""))
                                && ["exact_identifier", "reviewed_legacy_reference"]
                                    .contains(&r["evidence_kind"].as_str().unwrap_or(""))
                                && ["document_sha256", "document_payload_sha256", "plan_hash"]
                                    .iter()
                                    .all(|k| hash(&r[*k]))
                                && [
                                    "party_id",
                                    "evidence_json",
                                    "party_snapshot_json",
                                    "actor",
                                    "principal",
                                    "created_at",
                                ]
                                .iter()
                                .all(|k| r[*k].is_string())
                                && (r["reason"].is_null() || r["reason"].is_string())
                        }) && a
                            .windows(2)
                            .all(|p| p[0]["id"].as_i64() < p[1]["id"].as_i64())
                    } else {
                        a.len() <= 100
                            && a.iter().all(|r| {
                                exact(
                                    r,
                                    &[
                                        "id",
                                        "document_no",
                                        "sha256_hash",
                                        "linked",
                                        "resolution_state",
                                    ],
                                ) && id(&r["id"])
                                    && (r["document_no"].is_null() || r["document_no"].is_string())
                                    && hash(&r["sha256_hash"])
                                    && matches!(r["linked"].as_i64(), Some(0 | 1))
                                    && ["resolved", "unresolved", "internal_no_external_party"]
                                        .contains(&r["resolution_state"].as_str().unwrap_or(""))
                                    && (r["linked"] == 1) == (r["resolution_state"] == "resolved")
                            })
                            && a.windows(2)
                                .all(|p| p[0]["id"].as_i64() > p[1]["id"].as_i64())
                    }
                })
        }
    }
}
impl ApiClient {
    pub async fn document_party_original_list(
        &self,
        company: &str,
        status: Option<&str>,
    ) -> Result<Value, Error> {
        if status.is_some_and(|s| {
            ![
                "linked",
                "unlinked",
                "resolved",
                "internal_no_external_party",
                "unresolved",
            ]
            .contains(&s)
        }) {
            return Err(Error::InvalidInput("invalid resolution status"));
        }
        self.document_party_original_request(company, "", None, None, false, status)
            .await
    }
    pub async fn document_party_original_inspect(
        &self,
        company: &str,
        document_id: i64,
    ) -> Result<Value, Error> {
        if !(1..=9_007_199_254_740_991).contains(&document_id) {
            return Err(Error::InvalidInput("positive exact document id required"));
        }
        self.document_party_original_request(
            company,
            &format!("/{document_id}"),
            None,
            None,
            true,
            None,
        )
        .await
    }
    pub async fn document_party_original(
        &self,
        company: &str,
        op: DocumentPartyOriginalOperation,
        body: Vec<u8>,
        confirmed: bool,
    ) -> Result<Value, Error> {
        if op != Plan && !confirmed {
            return Err(Error::InvalidInput("confirmation required"));
        }
        document_party_original_validate(op, &body)?;
        let suffix = match op {
            Plan => "/plan",
            Apply => "/apply",
            Supersede => "/supersede",
            Internal => "/internal",
            InternalSupersede => "/internal-supersede",
        };
        self.document_party_original_request(company, suffix, Some(body), Some(op), false, None)
            .await
    }
    async fn document_party_original_request(
        &self,
        company: &str,
        suffix: &str,
        body: Option<Vec<u8>>,
        op: Option<DocumentPartyOriginalOperation>,
        inspect: bool,
        status: Option<&str>,
    ) -> Result<Value, Error> {
        let write = op.is_some_and(|o| o != Plan);
        let ambiguous = || {
            if write {
                Error::OutcomeUnknown
            } else {
                Error::Protocol
            }
        };
        let query = status.map(|s| vec![("status", s)]).unwrap_or_default();
        let mut response = self
            .send(
                if body.is_some() {
                    Method::POST
                } else {
                    Method::GET
                },
                &company_path(company, &format!("documents/original/party-links{suffix}"))?,
                &query,
                body,
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
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| ambiguous())? {
            if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE {
                return Err(ambiguous());
            }
            bytes.extend_from_slice(&chunk);
        }
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            if status == 422 {
                Error::Refused { status }
            } else {
                ambiguous()
            }
        })?;
        if status == 422 {
            return if curated(&value) {
                Ok(value)
            } else {
                Err(Error::Refused { status })
            };
        }
        if valid(&value, op, inspect) {
            Ok(value)
        } else {
            Err(ambiguous())
        }
    }
}
