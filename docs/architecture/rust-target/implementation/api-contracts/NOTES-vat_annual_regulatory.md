# VAT / annual / regulatory — #677 source contracts

## Delivery and evidence boundary

This partition contains all **60 frozen operation identities**, with `status: specified`, `unresolved: []`, all eight contract sections and partition-local Draft 2020-12 definitions. The recovered definitions were retained and extended rather than regenerating from the failed redacted command. The binary bound is represented numerically from `256 * 1024 * 1024`; no undefined redaction token remains.

- `python3 /tmp/r56-677-oracle.py vat_annual_regulatory`: **PASS**, 60 exact selected source-bound schema contracts. This validates frozen source hashes, identity coverage, references and the existing strict schema rules; it does **not** establish semantic acceptance.
- `graphify query 'vat annual regulatory'` ran before source inspection.
- `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`: completed, 34,003 nodes / 107,665 edges. Generated observations remain in the independent build output, not owned artifacts.
- No Cargo, server, browser, network, provider, authentication, posting, scanner or filesystem-publication flow was executed. Existing runtime/user-flow proofs were not replayed.

Main owns actual-source review, integration and acceptance. This delivery does not close #677, assert live provider approval or replace prior accepted runtime evidence. Prior 821 operations / 26 partitions and all55 / J01–J07 / final desktop / release / cutover / retirement remain unchanged.

## Transport and parser distinctions

### HTTP and HEAD

The selected source bytes in `annual_exports`, `annual_report`, `compliance`, `eu_sales`, `oss_report`, `vat` and `vat_original` do **not** contain an early HEAD fence. Their GET routing runs the actual handler and Axum strips response bytes. This is distinct from the already accepted R54 corrections in other handlers: no prospective 405 was projected into this partition. HEAD may therefore require current credentials, reject selectors, perform an admitted read or return a business refusal; it is not an unauthenticated successful GET alias.

Global manifest/version/profile and host/origin checks remain before leaves. Original VAT transports parse inside current authority; invalid transport is carried into the callback, not used to bypass principal checks. Native annual and VAT handlers retain their own guard order. Shared native VAT query accepts `source=live|archive` but this leaf does not use source; it always invokes live native VAT. This ignored HTTP selector does not make `remote vat --source` legal: the earlier generic remote report guard rejects it.

Original GET query is at most 1024 bytes, each decoded value at most 128 UTF-8 bytes, with no controls/replacement characters. Duplicate and unknown keys fail. Missing/empty from/to becomes empty domain strings. Content/transfer encoding and idempotency-key are forbidden; content-length if supplied is exactly 0; body is empty. Native EU/OSS require ordered real inclusive dates, query at most 128 bytes, no duplicate keys/bad percent escapes, no content encoding and an empty body. Annual explicit Gregorian bounds must be ordered and permit deriving a prior year. Company compliance accepts only literal `asOf=YYYY-MM-DD`, not percent-encoded alternatives or a reconstructed historical snapshot. Static regulatory handlers ignore leaf query/body and credentials; they use embedded repository bytes, not company authority.

Native export structs support complete declaration-order positional sequences as well as objects: SAF-T is periodStart, periodEnd, confirm, generationMode, generatedAt (five values); handoff is periodStart, periodEnd, confirm, packageProfile, requestedAt, requester (six). Optional string fields admit explicit null on direct HTTP. MCP still has an earlier object-only outer guard. Purchase's nested payload must be an object after the authority-fenced raw-value check; its optional actor uses a custom String deserializer, omitted allowed but explicit null rejected.

### CLI

Public argv first passes `main` / `local_binding`: split and attached global `--local-profile` are consumed and rewritten before leaf dispatch. The binding validates a private Unix transport and exact admitted origin; no TCP fallback, child server, discovery or impersonation. Leaf normalized flag maps are not the full public argv language. Earlier generic remote budget/report/source/service guards can reject a recognized flag before the report leaf. No new aliases or ignored CLI flags are implied by shared parsing.

Original VAT report commands accept their exact from/to forms, including filing and momsangivelse aliases. They render flat source JSON or source human output, choosing JSON for nonterminal stdout unless explicitly human. Native remote reads retain `{ok,data}` rather than replacing business-false nested data with a client error wrapper. Original annual/tax `--json` emits the report to stdout (business false exits 1); human business refusal uses stderr. Usage exits 2; business/transport exits 1; success exits 0. Original purchase invocation itself confirms; there is no `--confirm` flag. `--actor` is only a current credential assertion, never environment-derived authority.

Original `system export-saft` dispatches to `saft_artifact`, **not** the native remote `saft` stdin adapter. It chooses period_end absent `--generated-at`, caller_supplied when present. Native omitted generationMode instead uses trusted runtime time. Server normalizes explicit-zone timestamps to UTC milliseconds, refuses leap seconds and always evaluates authorization using runtime time.

### MCP

`tools::call` rejects non-object arguments before original and native dispatch. `vat_original::accepts` wins over native names: vat_report / vat_eu_sales_list / vat_oss_report use original company/from/to, not native year selectors. `vat` itself uses native optional year. Original tax preparation likewise uses company/from/to. Public RPC parse schemas enumerate actual selected reachable leaf messages plus session/framing errors, not startup argv diagnostics or unreachable helper branches.

Selected ordinary messages use the 1 MiB NDJSON frame limit. Session IDs accept UTF-8 strings at most 256 bytes or signed-i64/unsigned-u64 numbers; framing errors have id:null. Initialization and notifications/initialized precede tools. Notifications never execute tools. Result schemas refer to the JSON-RPC result member; content[0].text serializes structuredContent. Original tools keep errors/appliedRules/data projection conventions; native tools keep `{ok,data}` / client-error conventions. Native annual outer ok true does not assert nested report.ok true.

## Business, authority and recovery

- Original VAT business false is retained HTTP422 core; its dedicated SDK validates and retains that core. Native generic GET discards non-success bodies. Native HTTP200 outer ok true may retain failed annual/tax/iXBRL preparation. Business report success is not remote statutory filing.
- VAT purchase is **CompanyLedgerPost**, not a report. HTTP requires strict payload and confirm:true. Optional actor asserts current principal only. Invoice selector resolves newest matching document and overrides supplied documentId. Representation omission of expenseAccountNo requires a unique configured active REPRESENTATION_SPECIAL expense account; no fixed product account default.
- Raw HTTP purchase receipt entryId is SQLite signed i64; original SDK projection requires positive safeinteger. Document/bank selectors are positive safeintegers. Client validates 422 business core; 408/5xx, malformed success or response loss can become outcome_unknown. No caller key, TTL, tombstone or automatic replay guarantee exists. Read canonical journal/document/bank and reconcile original request before another action.
- Current Owner namespace/admission/schema/source assurance, current human/service authority, principal/company/ledger scope, transfer/backup/period and original/hash checks remain source-owned. Service usage and business stages are distinct. Current authority changing or a result being absent proves neither rollback nor noneffect. No blanket claim that reads/refusals have no metadata effects.
- Authority/accountant/SAF-T exports audit CompanyExport, verify originals and return tar. Original CLI handoff/SAF-T and MCP authority verify package hashes and materialize in the **invoking process filesystem**, fresh root/no overwrite. Server audit commit and local publication are separate. Lost response or local failure must not trigger automatic re-export.
- Annual `--ixbrl-out` verifies server XHTML hash and creates a new 0600 file without overwrite; failed publication attempts cleanup. Compliance `--out` is different: it explicitly writes HTML using the source write operation and does not inherit the package no-overwrite promise.
- Company compliance is current-state evidence. Its owner branch may read retained backups and verify signatures; nonowners do not receive those details. Remote synchronization and live provider governance are not established. Static coverage/citations are citation traceability / legacy attribution, not per-company compliance, statutory completeness, legal approval or native runtime coverage.

## Review focus

Definitions and rows are source declarations, not semantic test receipts. Main should independently review exact optional/correlated states, CLI stream variants, refusal status reachability and regulatory/audit component shapes against producers. The strict oracle intentionally does not prove these correlations or replace actual-source review. No other new parallel partition is referenced.
