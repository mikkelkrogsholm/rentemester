//! Finite, side-effect-free native workflow discovery. This is not authorization
//! or a claim that the entire legacy agent catalogue has been ported.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod bank;
pub mod original123;

const SCHEMA: &str = "rentemester-agent-discovery-v1";
const WORKFLOW: &str = "supplier-payable-handling";
const CAPABILITY: &str = "supplier-purchases";
const RULE_REASON: &str = "Native workflow metadata does not load or attest a TypeScript rule bundle; current server accounting validation remains authoritative.";
const PARTIAL: &str = "Only supplier-payable-handling and bank-reconciliation-batch are described. Other native capabilities may exist but are not covered by this catalogue; discovery is not full CLI/MCP/HTTP parity.";

fn build() -> Value {
    json!({"version":env!("CARGO_PKG_VERSION"),"runtime":"rust",
        "gitCommit":null,"builtAt":null,
        "reason":"Pure engine metadata has no packaged build-identity access; verify the running artifact separately."})
}
fn capability() -> Value {
    json!({"id":CAPABILITY,"title":"Supplier expenses and payables",
        "purpose":"Register reviewed supplier originals as open payables and record later bank settlement.",
        "domain":"purchases","outcomes":["register supplier payable","record supplier payment"],
        "keywords":["supplier","vendor","payable","purchase","expense","bank","VAT","kreditor","leverandør"],
        "scope":"company","supportStatus":"partial","maturity":"limited",
        "workflowIds":[WORKFLOW],"canonicalState":["documents","payables","bank transactions","journal entries"],
        "unsupportedBoundaries":[PARTIAL,"No bank transfer, supplier message, automatic payment or generic keyed retry is performed."]})
}
fn operation(method: &str, pattern: &str) -> Value {
    let mut value = json!({"surface":"http","method":method,"pattern":pattern,
        "id":format!("http:{method} {pattern}"),"resolved":null,
        "reason":"Resolve against the running native server; metadata is not runtime authorization or route attestation."});
    if method == "GET" {
        value["queryParameters"] = json!({"status":"all"});
        value["optionalArguments"] = json!({"asOf":"Explicit YYYY-MM-DD when needed; earlier dates omit later bills/payments. Omit to use current server date; do not use a historic date to reconcile a later write."});
    }
    value
}
fn step(id: &str, depends_on: &[&str], write: bool, tool: &str, cli: &str, pattern: &str) -> Value {
    json!({"id":id,"dependsOn":depends_on,"boundary":if write {"irreversible"} else {"read"},
        "operation":operation(if write {"POST"} else {"GET"},pattern),
        "aliases":[{"surface":"mcp","name":tool},{"surface":"cli","key":cli}],
        "purpose":"", "prerequisites":[],"inputIdentities":[],"outputIdentities":[],
        "expectedSafety":if write {"write"} else {"read"},"expectedIdempotent":!write,
        "requiresActor":write,"requiresConfirmation":write,
        "retryClass":if write {"unsafe-read-back"} else {"safe-read"},
        "canonicalRecords":["payables","bank transactions","journal entries","documents"]})
}
fn workflow() -> Value {
    let mut register = step(
        "register-payable",
        &[],
        true,
        "register_payable",
        "remote register-payable",
        "/api/companies/:slug/payables",
    );
    register["purpose"] = json!("Recognize an unpaid DKK supplier original; no payment occurs.");
    register["prerequisites"] = json!([
        "Select an authorized company and immutable original document; inspect document and expense_booking_options first.",
        "Review the supplier bill date and DKK amounts derived from that original.",
        "Choose an explicit expense account and current confirmed creditor and VAT roles; server rules determine admissibility.",
        "Current human or explicitly granted service authority and confirmed request body are required."
    ]);
    register["inputIdentities"] = json!([
        "company slug",
        "documentId",
        "expense account",
        "reviewed bill date"
    ]);
    register["outputIdentities"] = json!(["payableId", "journalEntryId"]);
    register["requiredArguments"] = json!(["company", "body", "confirm:true"]);
    register["bodyFields"] = json!({"required":["documentId","billDate","dueDate","expenseAccountNo","confirm"],
        "documentId":"positive original document ID","billDate":"reviewed YYYY-MM-DD matching original","dueDate":"reviewed YYYY-MM-DD",
        "expenseAccountNo":"explicit selected account number","confirm":true,
        "encoding":"HTTP JSON object; MCP body is this exact JSON encoded as a string plus outer confirm:true; CLI reads exact JSON on stdin with --confirm."});
    register["uncertainOutcomeReadBack"] = operation("GET", "/api/companies/:slug/payables");

    let mut list = step(
        "list-payables",
        &["register-payable"],
        false,
        "payables",
        "remote payables",
        "/api/companies/:slug/payables",
    );
    list["purpose"] =
        json!("Read canonical open balance and payable identity before choosing a payment.");
    list["readArguments"] = json!({"status":"all"});
    list["cliArguments"] = json!(["--status", "all"]);
    list["inputIdentities"] = json!(["company slug", "payableId"]);
    list["outputIdentities"] = json!([
        "payableId",
        "current open balance",
        "original registration journal"
    ]);
    list["prerequisites"] = json!([
        "Read canonical bank rows and account mapping separately; never infer payment from a proposed match."
    ]);

    let mut pay = step(
        "pay-payable",
        &["list-payables"],
        true,
        "pay_payable",
        "remote pay-payable",
        "/api/companies/:slug/payables/:id/pay",
    );
    pay["purpose"] = json!(
        "Record settlement against an existing outgoing DKK bank row; this never initiates a bank transfer."
    );
    pay["prerequisites"] = json!([
        "Review the current open balance and original registered creditor account.",
        "Choose an unused outgoing DKK bank row and its mapped bank ledger account.",
        "The entire row must fit the open balance; any supplied amount/date must exactly match the row and account must match bank mapping.",
        "Current authorized principal, writable company and explicit confirmation remain server-enforced."
    ]);
    pay["inputIdentities"] = json!([
        "company slug",
        "payableId",
        "bankTransactionId",
        "mapped bank ledger account"
    ]);
    pay["outputIdentities"] = json!([
        "payable payment identity",
        "journalEntryId",
        "bank reconciliation identity"
    ]);
    pay["requiredArguments"] = json!(["company", "id:payableId", "body", "confirm:true"]);
    pay["bodyFields"] = json!({"required":["bankTransactionId","confirm"],"bankTransactionId":"positive selected outgoing DKK bank row ID","confirm":true,
        "encoding":"HTTP JSON object; MCP body is this exact JSON encoded as a string plus outer confirm:true; CLI reads exact JSON on stdin with --confirm.",
        "payableIdentity":"HTTP route :id is the payableId; MCP uses positive id outside body; CLI uses --id PAYABLE_ID. Do not put payableId in the native body."});
    pay["uncertainOutcomeReadBack"] = operation("GET", "/api/companies/:slug/payables");

    let mut verify = step(
        "verify-payable",
        &["pay-payable"],
        false,
        "payables",
        "remote payables",
        "/api/companies/:slug/payables",
    );
    verify["purpose"] = json!(
        "Read canonical remaining balance and reconcile payment, bank, journal and original evidence."
    );
    verify["readArguments"] = json!({"status":"all"});
    verify["cliArguments"] = json!(["--status", "all"]);
    verify["prerequisites"] = json!([
        "Use status=all: the default open filter hides fully paid bills. Optional asOf must include the payment date; omit for current server date."
    ]);
    verify["inputIdentities"] = json!([
        "company slug",
        "payableId",
        "bankTransactionId",
        "journalEntryId"
    ]);
    verify["outputIdentities"] = json!(["remaining open balance", "recorded payment state"]);
    json!({"id":WORKFLOW,"capabilityId":CAPABILITY,"title":"Supplier payable handling",
        "intendedOutcome":"Register a supplier invoice as an open payable and record its later bank payment.",
        "nonGoals":["Discovery never executes, authorizes or confirms a write.","No automatic bank transfer, supplier communication or inferred accounting approval."],
        "prerequisites":["Verify running native build identity and current access.","Resolve operations and aliases on the selected live surface.","Use one explicit authorized company; preserve immutable source identities."],
        "steps":[register,list,pay,verify],
        "blockers":[{"code":"INPUT_VALIDATION","meaning":"Correct the named schema field only after establishing that the previous action was refused."},
            {"code":"AUTHORITY_OR_CONFIRMATION","meaning":"Obtain current company permission and explicit confirmation."},
            {"code":"BUSINESS_PRECONDITION","meaning":"Missing source, account role, bank mapping, open balance or write authority blocks accounting."}],
        "recovery":["An uncertain write must read payables, bank, journal and documents in the same company before further action.",
            "Stop automatic retry: these writes have no generic keyed receipt/replay contract.",
            "If canonical reads cannot establish the outcome, stop and require human reconciliation; never invent a new request key or assume failure.",
            "Use explicit correction/reversal controls for accepted accounting; never rewrite original evidence."],
        "stopConditions":["Required source, current authority or human confirmation is absent.","A referenced operation is unavailable on the chosen surface.",
            "A write outcome remains uncertain after canonical readback.","Bank currency, row ownership, mapping or current balance does not meet server rules.","The requested outcome exceeds this finite catalogue."],
        "relatedWorkflowIds":[],"alternatives":[],"unsupportedBoundaries":[PARTIAL,"No generic key-idempotent retry or automatic resend is promised."],
        "live":null,"unresolvedOperations":[],"resolutionReason":"Live route and alias presence must be checked by the selected transport; this pure metadata module does not resolve registries."})
}
// Sort object keys explicitly: serde_json feature unification may enable
// preserve_order in another transport, but must not change catalogue identity.
fn canonical(value: &Value) -> String {
    match value {
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        Value::Object(fields) => {
            let mut keys = fields.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!("{}:{}", json!(key), canonical(&fields[key])))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => value.to_string(),
    }
}
fn contents() -> Value {
    json!({"schemaVersion":SCHEMA,"capabilities":[capability(),bank::capability()],"workflows":[workflow(),bank::workflow()]})
}
fn catalogue() -> Value {
    let contents = contents();
    let hash = format!("{:x}", Sha256::digest(canonical(&contents).as_bytes()));
    json!({"schemaVersion":SCHEMA,"hash":hash,
        "entryPoint":"meta_about -> agent_capability_search -> agent_workflow_describe -> tools/list",
        "capabilityCount":contents["capabilities"].as_array().unwrap().len(),"workflowCount":contents["workflows"].as_array().unwrap().len(),"fullCoverage":false,
        "coverage":{"schemaVersion":"rentemester-agent-discovery-coverage-v1","fullCoverage":false,"scope":[WORKFLOW,bank::WORKFLOW],"reason":PARTIAL},
        "build":build(),"provenance":{"product":{"name":"rentemester","version":env!("CARGO_PKG_VERSION"),"runtime":"rust"},"scope":"finite native workflow metadata; verify packaged build identity separately"},
        "ruleBundleVersion":null,"ruleBundleVersionReason":RULE_REASON})
}
/// Native identification and finite catalogue. MCP may replace `toolCount` with
/// its actual registry count; the engine never claims to know that registry.
pub fn about() -> Value {
    let catalogue = catalogue();
    json!({"serverName":"rentemester","serverVersion":env!("CARGO_PKG_VERSION"),
        "toolCount":null,"ruleBundleVersion":null,"ruleBundleVersionReason":RULE_REASON,
        "build":catalogue["build"],"provenance":catalogue["provenance"],
        "contractDocs":["docs/mcp-agent-contract.md","docs/mcp-tool-surface.md","docs/confirm-contract.md","docs/cli-contract.md"],
        "catalogue":catalogue,"fullCoverage":false})
}
/// AND-match query tokens over each advertised capability. Character limits are
/// Unicode scalar counts; byte-heavy queries cannot evade the documented bound.
pub fn search(query: &str, cursor: usize, limit: usize) -> Result<Value, &'static str> {
    if query.chars().count() > 200 {
        return Err("query exceeds 200 characters");
    }
    if cursor > 1_000_000 {
        return Err("cursor exceeds 1000000");
    }
    if !(1..=50).contains(&limit) {
        return Err("limit must be between 1 and 50");
    }
    let matches = [capability(), bank::capability()]
        .into_iter()
        .filter(|capability| {
            let haystack = ["id", "title", "purpose", "domain", "outcomes", "keywords"]
                .iter()
                .map(|key| capability[*key].to_string())
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            query
                .split_whitespace()
                .all(|token| haystack.contains(&token.to_lowercase()))
        })
        .collect::<Vec<_>>();
    let total = matches.len();
    let items = matches
        .into_iter()
        .skip(cursor)
        .take(limit)
        .map(|mut capability| {
            capability.as_object_mut().unwrap().remove("keywords");
            capability.as_object_mut().unwrap().remove("canonicalState");
            capability["operations"] = if capability["id"] == CAPABILITY {
                json!([
                    operation("GET", "/api/companies/:slug/payables"),
                    operation("POST", "/api/companies/:slug/payables"),
                    operation("POST", "/api/companies/:slug/payables/:id/pay")
                ])
            } else {
                bank::operations()
            };
            capability
        })
        .collect::<Vec<_>>();
    let next = cursor + items.len();
    let more = next < total;
    Ok(
        json!({"catalogue":catalogue(),"total":total,"count":items.len(),"cursor":cursor,"limit":limit,"hasMore":more,"nextCursor":if more{json!(next)}else{Value::Null},"items":items}),
    )
}
pub fn describe(id: &str) -> Option<Value> {
    match id {
        WORKFLOW => {
            Some(json!({"catalogue":catalogue(),"capability":capability(),"workflow":workflow()}))
        }
        bank::WORKFLOW => Some(
            json!({"catalogue":catalogue(),"capability":bank::capability(),"workflow":bank::workflow()}),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_catalogue_identity_is_deterministic_and_honest() {
        let first = about();
        assert_eq!(first, about());
        assert!(first["toolCount"].is_null());
        assert!(first["ruleBundleVersion"].is_null());
        assert!(
            !first["ruleBundleVersionReason"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(first["fullCoverage"], false);
        assert_eq!(first["catalogue"]["capabilityCount"], 2);
        assert_eq!(first["catalogue"]["workflowCount"], 2);
        let digest = first["catalogue"]["hash"].as_str().unwrap();
        assert_eq!(digest.len(), 64);
        assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
        let contents = contents();
        assert_eq!(
            digest,
            format!("{:x}", Sha256::digest(canonical(&contents).as_bytes()))
        );
        assert_eq!(first["provenance"]["product"]["runtime"], "rust");
        assert_eq!(
            canonical(&json!({"z":1,"a":{"y":true,"b":null}})),
            "{\"a\":{\"b\":null,\"y\":true},\"z\":1}"
        );
    }
    #[test]
    fn bounded_search_and_pagination_never_invent_more_capabilities() {
        let found = search("SUPPLIER payable", 0, 1).unwrap();
        assert_eq!(found["count"], 1);
        assert_eq!(found["items"][0]["id"], CAPABILITY);
        assert_eq!(found["hasMore"], false);
        assert!(found["nextCursor"].is_null());
        let first = search("", 0, 1).unwrap();
        assert_eq!(first["total"], 2);
        assert_eq!(first["count"], 1);
        assert_eq!(first["hasMore"], true);
        assert_eq!(first["nextCursor"], 1);
        let second = search("", 1, 1).unwrap();
        assert_eq!(second["items"][0]["id"], bank::CAPABILITY);
        assert_eq!(second["hasMore"], false);
        assert!(second["nextCursor"].is_null());
        assert_eq!(search("", 2, 50).unwrap()["count"], 0);
        let bank = search("BANK bookkeeping", 0, 50).unwrap();
        assert_eq!(bank["count"], 1);
        assert_eq!(bank["items"][0]["id"], bank::CAPABILITY);
        assert_eq!(bank["catalogue"]["hash"], first["catalogue"]["hash"]);
        assert_eq!(search("", 1_000_000, 1).unwrap()["count"], 0);
        assert_eq!(search("not-a-real-domain", 0, 1).unwrap()["total"], 0);
        assert!(search(&"ø".repeat(200), 0, 1).is_ok());
        assert!(search(&"ø".repeat(201), 0, 1).is_err());
        assert!(search("", 1_000_001, 1).is_err());
        assert!(search("", 0, 0).is_err());
        assert!(search("", 0, 51).is_err());
        assert!(describe("supplier-expense-booking").is_none());
        assert!(describe("").is_none());
    }
    #[test]
    fn writes_are_confirmed_and_require_readback_not_automatic_keyed_retry() {
        let description = describe(WORKFLOW).unwrap();
        let steps = description["workflow"]["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 4);
        for step in steps {
            let write = step["expectedSafety"] == "write";
            assert_eq!(step["expectedIdempotent"], !write);
            assert_eq!(step["requiresConfirmation"], write);
            assert_eq!(step["requiresActor"], write);
            assert_eq!(
                step["retryClass"],
                if write {
                    "unsafe-read-back"
                } else {
                    "safe-read"
                }
            );
            assert!(step["operation"]["resolved"].is_null());
            if write {
                assert_eq!(step["uncertainOutcomeReadBack"]["method"], "GET");
            }
        }
        assert_eq!(steps[0]["aliases"][0]["name"], "register_payable");
        assert_eq!(steps[2]["aliases"][0]["name"], "pay_payable");
        assert_eq!(steps[2]["dependsOn"], json!(["list-payables"]));
        assert_eq!(steps[3]["readArguments"]["status"], "all");
        assert_eq!(
            steps[0]["uncertainOutcomeReadBack"]["queryParameters"]["status"],
            "all"
        );
        assert_eq!(
            steps[2]["bodyFields"]["required"],
            json!(["bankTransactionId", "confirm"])
        );
        assert!(
            description["workflow"]["recovery"]
                .to_string()
                .contains("Stop automatic retry")
        );
    }
    #[test]
    fn native_bank_journey_preserves_source_review_dependencies_and_exact_hashes() {
        let description = describe(bank::WORKFLOW).unwrap();
        let steps = description["workflow"]["steps"].as_array().unwrap();
        assert_eq!(steps.len(), 13);
        let mut seen = std::collections::BTreeSet::new();
        for step in steps {
            let id = step["id"].as_str().unwrap();
            for dependency in step["dependsOn"].as_array().unwrap() {
                assert!(
                    seen.contains(dependency.as_str().unwrap()),
                    "{id} dependency must precede it"
                );
            }
            assert!(seen.insert(id));
            let write = step["expectedSafety"] == "write";
            assert_eq!(step["requiresConfirmation"], write);
            assert_eq!(step["requiresActor"], write);
            assert_eq!(step["expectedIdempotent"], !write);
            assert_eq!(
                step["retryClass"],
                if write {
                    "unsafe-read-back"
                } else {
                    "safe-read"
                }
            );
            assert!(step["operation"]["resolved"].is_null());
            if write {
                assert_eq!(
                    step["mcpInputSchema"]["properties"]["confirm"]["const"],
                    true
                );
                assert_eq!(step["bodySchema"]["properties"]["confirm"]["const"], true);
                assert_eq!(step["uncertainOutcomeReadBack"]["method"], "GET");
            }
        }
        let get = |id: &str| steps.iter().find(|s| s["id"] == id).unwrap();
        assert_eq!(
            get("persist-batch")["authority"]["permission"],
            "CompanyDraftWrite"
        );
        assert_eq!(
            get("approve-batch")["authority"]["permission"],
            "CompanyReview"
        );
        assert_eq!(
            get("apply-batch")["authority"]["permission"],
            "CompanyLedgerPost"
        );
        assert!(
            get("approve-batch")["prerequisites"]
                .to_string()
                .contains("different from the stored planner")
        );
        assert_eq!(
            get("persist-batch")["bodySchema"]["required"],
            json!([
                "accountingFrom",
                "accountingTo",
                "bankFrom",
                "bankTo",
                "runKey",
                "expectedPlanHash",
                "confirm"
            ])
        );
        for id in ["approve-batch", "apply-batch"] {
            assert_eq!(
                get(id)["bodySchema"]["required"],
                json!(["runId", "planHash", "confirm"])
            );
        }
        assert_eq!(get("apply-batch")["dependsOn"], json!(["approve-batch"]));
        assert!(
            get("apply-batch")["prerequisites"]
                .to_string()
                .contains("atomic_v1")
        );
        assert!(
            get("apply-batch")["replayContract"]
                .as_str()
                .unwrap()
                .contains("no automatic resend")
        );
        assert!(
            description["workflow"]["recovery"]
                .to_string()
                .contains("inspect-only")
        );
        let original = contents();
        let mut changed = original.clone();
        changed["workflows"][1]["steps"][6]["bodySchema"]["required"] = json!(["wrong"]);
        assert_ne!(
            Sha256::digest(canonical(&original).as_bytes()),
            Sha256::digest(canonical(&changed).as_bytes())
        );
    }
    #[test]
    fn native_bank_operations_have_exact_names_and_clean_route_specific_queries() {
        let description = describe(bank::WORKFLOW).unwrap();
        let steps = description["workflow"]["steps"].as_array().unwrap();
        let get = |id: &str| steps.iter().find(|s| s["id"] == id).unwrap();
        assert_eq!(get("import-bank")["aliases"][0]["name"], "import_bank");
        assert_eq!(
            get("import-bank")["aliases"][1]["key"],
            "remote import-bank"
        );
        assert_eq!(get("inspect-bank")["aliases"][0]["name"], "bank");
        assert_eq!(
            get("inspect-suggestions")["aliases"][0]["name"],
            "bank_suggest_matches"
        );
        assert_eq!(
            get("inspect-source-queue")["aliases"][0]["name"],
            "reviewed_bank"
        );
        assert_eq!(
            get("plan-batch")["mcpInputSchema"]["required"],
            json!(["company", "scope"])
        );
        assert_eq!(
            get("plan-batch")["operation"]["querySchema"]["required"],
            json!(["accountingFrom", "accountingTo", "bankFrom", "bankTo"])
        );
        assert_eq!(
            get("persist-batch")["uncertainOutcomeReadBack"]["pattern"],
            "/api/companies/:slug/bookkeeping-batch/runs"
        );
        assert_eq!(
            get("persist-batch")["uncertainOutcomeReadBack"]["queryParameters"]["runKey"],
            "exact original caller runKey"
        );
        assert_eq!(
            get("inspect-stored-run")["mcpInputSchema"]["oneOf"],
            json!([{"required":["runId"]},{"required":["runKey"]}])
        );
        assert_eq!(
            get("verify-journal")["operation"]["queryParameters"],
            json!({"source":"live"})
        );
        let import = &get("import-bank")["bodySchema"];
        assert_eq!(import["required"], json!(["confirm"]));
        assert_eq!(import["oneOf"].as_array().unwrap().len(), 2);
        assert_eq!(
            get("import-bank")["mcpInputSchema"]["properties"]["body"]["maxLength"],
            35 * 1024 * 1024
        );
        for step in steps {
            for key in ["operation", "uncertainOutcomeReadBack"] {
                if let Some(op) = step.get(key) {
                    assert!(op["queryParameters"].get("status").is_none());
                    assert!(op["queryParameters"].get("asOf").is_none());
                    assert!(!op["pattern"].as_str().unwrap().contains('?'));
                }
            }
            for alias in step["aliases"].as_array().unwrap() {
                assert_ne!(alias["name"], "bank_list");
                assert_ne!(alias["name"], "bank_import");
            }
        }
        let operations = bank::operations();
        assert_eq!(operations.as_array().unwrap().len(), 12);
        let mut ids = std::collections::BTreeSet::new();
        for op in operations.as_array().unwrap() {
            assert!(ids.insert(op["id"].as_str().unwrap()));
        }
        assert_eq!(description["catalogue"]["fullCoverage"], false);
        assert_eq!(
            description["catalogue"]["coverage"]["scope"],
            json!([WORKFLOW, bank::WORKFLOW])
        );
    }
}
