//! Source dimension tools, sharing the frozen SDK schema and authority boundary.
use rentemester_client::{
    ApiClient, DimensionsOriginalOperation as Operation, Error, original_dimensions_schema,
    validate_original_dimensions,
};
use serde_json::{Value, json};

const BINDINGS: [(&str, Operation); 14] = [
    ("dimension_definition_create", Operation::Define),
    ("dimension_member_create", Operation::Member),
    (
        "dimension_definition_lifecycle",
        Operation::DefinitionLifecycle,
    ),
    ("dimension_member_lifecycle", Operation::MemberLifecycle),
    ("dimension_definition_list", Operation::Definitions),
    ("dimension_member_list", Operation::Members),
    ("dimension_assignment_plan", Operation::Plan),
    ("dimension_assignment_apply", Operation::Apply),
    ("dimension_assignment_replace", Operation::Replace),
    ("dimension_assignment_supersede", Operation::Supersede),
    ("dimension_assignment_list", Operation::List),
    ("dimension_budget_plan", Operation::BudgetPlan),
    ("dimension_budget_apply", Operation::BudgetApply),
    ("dimension_budget_list", Operation::Budgets),
];
fn operation(name: &str) -> Option<Operation> {
    BINDINGS.iter().find(|(n, _)| *n == name).map(|(_, op)| *op)
}
pub fn accepts(name: &str) -> bool {
    operation(name).is_some()
}
fn read(op: Operation) -> bool {
    matches!(
        op,
        Operation::Definitions
            | Operation::Members
            | Operation::Plan
            | Operation::List
            | Operation::BudgetPlan
            | Operation::Budgets
    )
}
fn schema(op: Operation) -> Value {
    let mut schema = original_dimensions_schema(op);
    schema["properties"]["company"] = crate::tools::company_schema();
    let required = schema["required"].as_array_mut().unwrap();
    required.retain(|key| key != "confirm");
    required.insert(0, json!("company"));
    if !read(op) {
        // Source discovery permits omission; execution requires affirmative consent.
        schema["properties"]["confirm"] = json!({"type":"boolean"});
    }
    schema
}
pub fn tools() -> Vec<Value> {
    BINDINGS.into_iter().map(|(name, op)| {
        let description = match op {
            Operation::Plan => "Read-only exact hash-bound classification plan for an immutable journal line. Each dimension reconciles to the whole line. Business refusal is an outer failure, not a successful plan.",
            Operation::BudgetPlan => "Read-only exact allocation plan against the existing account/month budget and current active members. Business refusal is an outer failure.",
            Operation::Definitions | Operation::Members | Operation::List => "Read append-only dimension history, preserving original source evidence and historical labels.",
            Operation::Budgets => "Read current reviewed dimension budget allocations ordered by period/account/dimension/member.",
            _ => "Confirmed append-only dimension mutation; no journal changes. Requires confirm:true before HTTP. Preserve the exact reviewed plan, expected current assignment and reason for replacement. Attribution derives only from current credentials. No automatic retries; reconcile outcome_unknown with canonical reads and the original caller key.",
        };
        let mut tool = crate::tools::tool(name, description, schema(op), read(op));
        tool["annotations"] = json!({"readOnlyHint":read(op),"destructiveHint":false,"idempotentHint":read(op) || matches!(op, Operation::BudgetApply),"openWorldHint":false});
        tool["outputSchema"] = json!({"type":"object","required":["ok","errors"],"properties":{"ok":{"type":"boolean"},"errors":{"type":"array","items":{"type":"string"}},"code":{"type":"string"},"data":{"type":"object"}},"additionalProperties":false});
        tool
    }).collect()
}
fn result(envelope: Value) -> Value {
    json!({"isError":envelope["ok"]!=true,"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope})
}
fn wrap(op: Operation, mut core: Value) -> Value {
    let ok = core["ok"] == true;
    let mut envelope = json!({"ok":ok,"errors":core.get("errors").cloned().unwrap_or(json!([]))});
    if ok {
        if matches!(op, Operation::Plan | Operation::BudgetPlan) {
            // Unlike lists/writes, source successEnvelope retains the entire core plan result.
            envelope["data"] = core;
        } else {
            let fields = core.as_object_mut().unwrap();
            fields.remove("ok");
            fields.remove("errors");
            envelope["data"] = core;
        }
    } else if let Some(code) = core.get("code") {
        envelope["code"] = code.clone();
    }
    result(envelope)
}
fn transport_error(error: Error) -> Value {
    let code = match error {
        Error::InvalidInput(_) => "invalid_input",
        Error::Refused { .. } => "server_refused",
        Error::Transport => "transport",
        Error::Protocol => "protocol",
        Error::OutcomeUnknown => "outcome_unknown",
    };
    result(json!({"ok":false,"code":code,"errors":[error.to_string()]}))
}
pub async fn call(api: &ApiClient, name: &str, mut args: Value) -> Result<Value, &'static str> {
    let op = operation(name).ok_or("unknown dimensions tool")?;
    let fields = args
        .as_object_mut()
        .ok_or("expected dimensions arguments object")?;
    let company = fields.remove("company").ok_or("company required")?;
    let company = company
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
        .ok_or("invalid company slug")?;
    // Identity assertions are intentionally not part of the MCP surface.
    if args.get("actor").is_some() || args.get("principal").is_some() {
        return Err("unexpected dimensions identity assertion");
    }
    if !read(op) {
        if args.get("confirm").is_some_and(|v| !v.is_boolean()) {
            return Err("confirm must be boolean");
        }
        if args["confirm"] != true {
            return Ok(result(
                json!({"ok":false,"code":"CONFIRM_REQUIRED","errors":[format!("confirm: true required for write tool {name}")]}),
            ));
        }
    }
    validate_original_dimensions(op, &args).map_err(|_| "invalid original dimensions arguments")?;
    Ok(
        match api
            .original_dimensions(company, op, &args, None, None)
            .await
        {
            Ok(core) => wrap(op, core),
            Err(error) => transport_error(error),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_envelopes_and_deliberate_plan_failure_correction() {
        for op in [Operation::Plan, Operation::BudgetPlan] {
            let core = json!({"ok":true,"plan":{"planHash":"a".repeat(64)}});
            let success = wrap(op, core.clone());
            assert_eq!(success["structuredContent"]["data"], core);
            assert_eq!(success["structuredContent"]["errors"], json!([]));
            assert_eq!(success["isError"], false);
            let failure = wrap(op, json!({"ok":false,"errors":["inactive member"]}));
            assert_eq!(failure["isError"], true);
            assert_eq!(
                failure["structuredContent"],
                json!({"ok":false,"errors":["inactive member"]})
            );
            assert_eq!(
                serde_json::from_str::<Value>(failure["content"][0]["text"].as_str().unwrap())
                    .unwrap(),
                failure["structuredContent"]
            );
        }
        for (op, key) in [
            (Operation::Definitions, "definitions"),
            (Operation::Members, "members"),
            (Operation::List, "assignments"),
            (Operation::Budgets, "budgets"),
        ] {
            let mut core = json!({"ok":true});
            core[key] = json!([]);
            let value = wrap(op, core);
            assert_eq!(value["structuredContent"]["data"][key], json!([]));
            assert!(value["structuredContent"]["data"].get("ok").is_none());
        }
        let value = wrap(
            Operation::Apply,
            json!({"ok":true,"id":7,"idempotent":false,"planHash":"a".repeat(64)}),
        );
        assert_eq!(value["structuredContent"]["errors"], json!([]));
        assert!(value["structuredContent"]["data"].get("ok").is_none());
        assert!(
            wrap(
                Operation::Apply,
                json!({"ok":false,"errors":["stale hash"]})
            )["structuredContent"]
                .get("data")
                .is_none()
        );
    }
}
