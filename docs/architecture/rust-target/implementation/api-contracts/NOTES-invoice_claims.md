# Invoice claims — #677 selected contracts

## Delivery and evidence

67 exact selected operation IDs: 26 HTTP, 25 CLI, 16 MCP. The four owned files contain operation contracts, Draft 2020-12 definitions, current source hashes/constructor needles and these review notes. All rows are `specified` with `unresolved: []`; that declaration remains subject to the main agent's independent semantic review, not an assertion of whole-issue acceptance.

Graphify query ran before source investigation. The callchain review covered protocol/session and CLI preprocessing before family dispatch, client transport, HTTP admission, application authority/source fences, storage constructors and engine policy. No HEAD probe, Cargo build, server, browser, network/provider request, financial effect or accepted runtime/user-flow replay was performed. Previously accepted 419 operations/18 partitions were not edited.

Existing check executed:

```text
python3 /tmp/r53-677-oracle.py invoice_claims
PASS exactselected invoice_claims 67 resolved sourcebound schema contracts; semanticreview required; whole677 remains open
```

The first oracle execution rejected required-only conditional selector schemas. Those were replaced with explicit object properties/additionalProperties constraints; subsequent executions passed. This is structural/sourcehash evidence, not live-provider approval or runtime acceptance.

Required AST extraction completed successfully:

```text
graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"
34009 nodes, 107681 edges; no clustering
```

Generated observations are in the independent build directory, outside owned code.

## Important source distinctions for integration

- **Selected reminder HEAD is not a safe read probe.** `product_http/reminder_claims.rs` classifies every non-GET as mutation. HEAD requires mutation Origin protection and JSON framing; its route suffix falls through to `Settle`, not `Read`. It can therefore reach the settlement permission/writer with a valid body. Axum strips response entity bytes, not the underlying dispatch. This is source-observed only. The parent-fixed native payables HEAD guards were neither changed nor replayed. Main should review this bounded finding before freezing source-dependent acceptance; these artifacts do not assert HEAD safety.
- Native compensation, interest and bad-debt HTTP handlers reject idempotency-key headers, but the reminder handler does not explicitly reject them. None of these selected operations consumes a client idempotency key or promises replay success.
- Native direct HTTP Serde accepts full declaration-order sequences as well as objects. MCP `tools::call` rejects non-object outer arguments before all family dispatch. SDK native body validation is object-based; direct HTTP tuples are not public MCP arguments.
- Shared compensation/interest MCP names select the original flat adapter when `body` is absent and native body-string adapter when present. Null/mixed bodies cannot fall back to the original branch. Bad-debt uses the same absence discriminator. Flat interest requires an explicit `referenceRate` in MCP even though original CLI can omit it for server statutory-table behavior.
- Original claim transport retains complete bounded validated HTTP 422 business receipts, including exact server code/errors; HTTP 200 business false is also retained. Generic native and original bad-debt client transport discard 422 bodies. Read-only native compensation/interest calculations map unknown-write transport classification back to Transport; other unknown writes remain uncertain.
- Original claims use lifecycle envelopes, whereas original bad-debt returns its enriched flat native result. Original claims CLI flattens successful data and has different human/JSON failure streams from original bad-debt. Native CLI preserves the HTTP envelope nested in `data`; native MCP adds its tool-result wrapper, not a standardized original result.
- Claim registration/posting/settlement sends no email. Compensation and interest remain separate DKK claims; original-currency principal and actual DKK carrying/VAT are distinct. Foreign `claimOpenBalance` is null. Native raw IDs are signed i64 producers; bounded positive safe input IDs do not narrow stored producer IDs.
- Registration is distinct from posting. A mathematical correction proposal does not imply `canPost`; refund/capacity/original-account/causal checks remain authoritative. Settlement consumes existing incoming imported DKK bank evidence only after principal settlement and pending-claim checks. Bad-debt loss leaves claims separate and does not establish legal evidence for the operator.
- Current control/company authority, verified originals, archived bank source hashes, current posting policy, transfer/backup/period controls and audit principal evidence remain required. Original adapters verify period sources too. No universal no-effects statement applies to refusal or service admission: usage metadata and business commits are separate. Changed authority, absent readback or transport failure is not rollback evidence.

## Scope retained

Only the four `invoice_claims` files are owned/changed. No new inventory, harness, dependency, production fix or protocol file. Whole #677, all 55 scope, J01–J07, final desktop acceptance, release/cutover and retirement remain at their actual prior scope; main owns review, integration and acceptance. No commit, push, merge, issue status change or delegation was performed.

Lead source correction: all4 interest correction outputs require outstandingClaimBalance, possibleOutstandingInterestReceivable and ambiguousInterestReceivable numbers. Native CLI/MCP content schemas now describe effective SDK validation separately from direct HTTP serde: reminder domain validator does not enforce advertised date patterns, accepts nullable amount but not present-null dates/accounts; other claim adapters independently validate real calendars and duplicate fields. Byte/Unicode checks are explicit annotations, not false character-count claims. Originalforms andpre-fixHEADSettle remain source-correct; methodguard must be fixed before external publication. No financial/provider effects replayed.
