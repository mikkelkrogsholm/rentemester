# #677 bank intake/accounts — source contracts after lead repair

## Scope and evidence

Graphify query was run first (`bank import bank accounts reconciliation`). Current Rust application, HTTP handlers, storage producers, SDK transports, CLI global/leaf parsing and MCP global/leaf dispatch were read. No Cargo, server, network, financial, provider, HEAD probe or accepted-proof replay was performed. Production and prior accepted partitions were not edited.

Lead repair now supplies all35 selected operation rows and corrects the source-bound receipt/service/decoder differences below. Parent semantic review/integration/acceptance remains required; whole #677/all55/J01–J07/final desktop/release/cutover/retirement scope remains unchanged.

## Historical worker blocker (superseded by lead completion)

Stopped generation after the two permitted setup failures, per work-package stop rule:

1. Operation-generation Python script failed at parse time: `SyntaxError: positional argument follows keyword argument` in the refusal human-text constructor (`'stderr': ...` used after keyword arguments). No operation artifacts were written by that attempt.
2. Corrected generation reached source-anchor validation and failed with `AssertionError: ('crates/rentemester-engine/src/bank.rs', 'pub fn decode_bytes(')`. The engine decode producer was read, but this exact literal anchor does not occur in the current source (declaration contains a different signature spelling). The operation-generation script only writes at its end, so no operations JSONL was produced. No script/new harness was installed.

Preserved partial `schemas-bank_intake_accounts.json` and `provenance-bank_intake_accounts.json` for lead repair. The missing operations file is intentional evidence of the blocker, not fabricated specified coverage. Existing strict oracle cannot pass without it.

## Useful source findings for repair/review

- `product_http/bank.rs::handle` treats non-POST as reads. Native bank/bank-accounts HEAD is not payables' early405 contract. `query_handle` first verifies credential and selected company, then rejects non-GET as400 invalid_query; Axum strips HEAD bodies. No HEAD requests were issued.
- `tools::call` rejects non-object arguments before native/original dispatch; no public MCP positional arrays. Direct HTTP native AccountInput derives struct tuple grammar in declaration order. Import custom csv string deserializers reject explicit null, while account/profile Option accepts null. Source exclusivity is enforced after decoding.
- Native account creation returns only id/slug; original add/update returns original errors/data.account envelope and uses CompanyMasterData versus native CompanyAdmin. Browser PATCH null strings clear, original commands reject null; omitted updates remain unchanged, slug immutable.
- Native bank reads stored raw_json through serde_json::Value: historical raw is genuinely broader than the current profile writer's string map. The partial schema scopes arbitrary JSON only to this raw field.
- Original transactions are date DESC,id DESC and optionally paginated; original CLI intentionally unpaged, MCP defaults500/0. Reconciliation reverses ordering and computes signed original-unit totals that can mix currencies. Reversal-linked rows become unmatched.
- Original import file adapter keeps exact bytes; nonempty MCP csvPath wins without inline fallback even if unreadable. Generic UTF8/BOM and declared danske-bank Windows1252/DMY differ from a generic upload receipt. Source publication precedes SQL import commit.
- Selected SDK non2xx responses discard bank-family422 bodies. Write5xx/lost/malformed success remains outcome_unknown. Original HTTP200 businessfalse is retained; native CLI/MCP preserve nested core data rather than standardizing to original envelopes. No automatic retries/caller keys.

## Historical partial-schema cautions

The partial schema is not semantically frozen. Import full positional parse versus semantic exclusivity, original Wire default tuple tails, raw ID versus SDK projections, exact reachable partition RPC messages, native statement status `known` (not `available`), per-status header/body correlations and actual CLI streams still require completion. Do not treat structurally present partial definitions as successful exact-contract acceptance.

## Lead completion

The original incomplete worker receipt above is preserved. Lead recovered the intended35 operation rows by fixing the exact engine decode re-export anchor, not restarting a model. Source status is known; original import omits null account/profile keys and has a separate receipt. Native service create/PATCH success is not advertised; read usage permits not_committed or committed and CLI/MCP retain supported service envelopes. Original Wire decoder sequences are distinguished from later command/null/selector validation. Strict checks and semantic review are required before integration; no runtime/provider/financial effects were replayed.
