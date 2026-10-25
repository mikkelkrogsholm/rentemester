# Voucher party / extraction / expense — #677 scoped contracts

52 frozen operation identities; status specified describes the source contracts, not whole-issue acceptance. Main owns semantic review/integration. No production, dependency, inventory, protocol or accepted partition changes. No runtime/provider/network/HEAD probe performed.

## Source chain and public boundaries

Graphify query was run before source inspection. CLI main executes original_args::prepare_with_output, family preflight, local_binding::prepare and the existing family/remote dispatcher. Original attached/split options and format normalization must not be confused with normalized transport flags. A private verified local profile installs exact-origin Unix transport, no TCP fallback. Original workspace/company path flags assert a verified binding, not another ledger selection. Native extraction allows only cookie authentication even though original extraction supports service credentials. Native match CLI is rejected by remote service allowlist; direct HTTP service dispatch reaches application and refuses unsupported Action::Post. Native options support the service read adapter.

MCP Session validates one JSON-RPC object, ID and session readiness; tools::call rejects non-object arguments BEFORE any original/native branch. Native leaf Serde sequences are not public MCP tuple arrays. Selected RPC errors enumerate actual code/message pairs in RpcError, including leaf validation. Text content is serialized structuredContent, not a separate result. Original tools retain flat envelopes and CONFIRM_REQUIRED result semantics; native adapters retain complete HTTP envelopes nested in data and native client error objects.

HTTP protocol middleware negotiates current advertised exact method/path before handlers; three absent headers preserve legacy-v1. Credential/origin/current company admission is not replaced by protocol negotiation. Native party endpoints are human-only. Original parties/extraction/expense accept supported typed service credentials; server actor/principal wins over equality assertions. Service usage admission may write metadata even for business reads/refusals. No blanket no-effects claim.

## HTTP parsing, status and HEAD

All selected HEAD identities are GET-fallback advertisements. These particular handlers branch POST versus non-POST, not GET-only and not the unrelated seven early HEAD guards. HEAD performs the read admission/storage path and Axum strips the body, including error bodies. No effect probes were performed. Successful read branch status is200; parse/authority/protocol/storage status remains authoritative; HEAD cannot replace JSON recovery readback.

Party original POST accepts exactly plan/apply/supersede/internal/internal-supersede, native exactly plan/apply/supersede; {action} remains ONE operation ID. Unknown actions404 not_found after earlier admission. POST requires exact application/json (case-insensitive), no query/content-encoding,16KiB limit; oversized413, malformed/unknown400 invalid_input. Original non-plan first requires object, validates document positive safe integer, lowercase hex hashes, nonblank bounded strings and role/evidence enums, strips expectedActor/expectedPrincipal, requires confirm:true (400 CONFIRMATION_REQUIRED), then typed deserialization. Plan parsed Value can admit full native PlanInput positional sequence at the final Serde boundary only if earlier documentId check succeeds (arrays fail that check); therefore original HTTP remains object-only. Native typed structs accept complete positional sequences; nullable Option strings differ from original optional non-null input guard. Native apply sequence order follows actual declaration: documentId,role,partyId,jurisdiction,identifierKind,identifier,legacyKind,legacyId,reviewedLegacyReference,planHash,confirm,idempotencyKey. Object duplicate JSON Value keys collapse in original parser and native expense map parser; typed direct native party duplicates are rejected. Party list query has only status (native also limit), duplicates/unknowns400 invalid_query. Original {action} GET/HEAD actually parses action as positive safe numeric document ID; otherwise400 invalid_document. No body consumption on party GET/HEAD. Original history removes idempotency_key; original supersede removes planHash. Native keeps both. Admitted business plan refusal is200 {ok:false,errors:[code]}; mutation/storage failure uses documents::outcome:401 unauthorized for invalid_credentials,403 access_denied only NotCommitted,422 exact code/message when NotCommitted,503 outcome_unknown otherwise. Join/owner failure503 outcome_unconfirmed. Service failures retain usage/business and map unknown/nonreusable503, invalid credentials401/access403, rejection/not_committed422, otherwise400.

Extraction POST native typed Confirm accepts object or exact [true]; original Confirm accepts object or [true,expectedActor-string] (present null is rejected even in sequence form). Original custom expectedActor present null is rejected in object form; omitted defaults None. Both require application/json, reject query/content-encoding/idempotency-key, body4096 bytes, positive safe path id. Read body must be empty (bounded1 byte), otherwise400; oversize POST413. Native malformed/false confirmation400 confirmation_required; original malformed400 invalid_input, false400 confirmation_required. Original unknown fields/duplicates rejected. Credential precedes selection (404 company_not_found) and parser. Worker unavailable503 busy. Native extraction failures: OutcomeUnknown503; invalid_credentials401; access_denied403; document_not_found404; provider/snapshot/storage unavailable503; invalid_input400; others422, body {ok:false,code,errors:[message]}. Original mapping is intentionally narrower: authority401/403 errors:[EXTRACTION_FAILED]; unknown/nonreusable/retired503 errors:[outcome_unknown]; provider-disabled503 errors:[EXTRACTION_PROVIDER_UNAVAILABLE]; snapshot invalid/hash/unavailable/admission-changed422 exact safe code in errors; other422 EXTRACTION_FAILED. Other common refusal constructors retain {ok:false,code,errors}. Successful extraction200 {ok:true,extraction:null|surface}; needs_resolution is success evidence, not accounting.

Original expense uses strict map deserializer (duplicate fields rejected), no query/content-encoding/idempotency-key header, exact JSON,16KiB write, empty read body, positive safe path IDs. confirm required/false maps400 invalid_input/confirmation_required respectively. Booking fields documentId,bankTransactionId,expenseAccount required; strings <=4096 bytes, text permits empty/control characters while other strings require nonblank/no controls. HTTP accepts all six VAT treatments; MCP expense_book intentionally excludes eu_goods_acquisition; CLI expense book accepts six but exposes no request key. Optional body idempotencyKey local SDK max128; HTTP initial string guard is4096 before storage key validation. Authority401/403 PURCHASE_FAILED; unknown503 outcome_unknown; known key conflict/expiry422 with exact code/message; invalid_input400; VIES config503; unavailable evidence/admission-changed422 exact errors; other422 purchase_refused. Definite preflight business false still200 with safe evidence/exception retained. Original adapter checked stage/refusal differs from native documents outcome. Native direct booking requires JSON object and confirm:true, removes it then Input Serde; POST16KiB. Options read no query, selected mismatch400 invalid_input, invalid path404 document_not_found. Native success200 {ok:true,booking:core} or {ok:true,options:core}; successful financial booking has required numeric gross/net/VAT/FX amounts and journal IDs/hash. Native service direct post is not supported, despite dispatch being reachable.

## Producer and SDK distinctions

Party snapshot comes from current capture of company-scoped identifiers/roles/history. Role defaults permit arbitrary Value under four allowed field names; original client reader accepts only scalars. This mismatch is documented rather than pretending the SDK schema bounds the producer. Historical evidence_json and party_snapshot_json stay strings on history reads. Extraction fields are rebuilt from retained SQL and validated by engine::fields; selected amount/date/currency/legal-ID values have extra constraints, other cited values are genuine arbitrary JSON. Native producer permits those values; original SDK scalar projection can report uncertain/protocol on nested values. SourceText validation counts trimmed UTF16 units <=4000, not raw JSON-schema maxLength. Currency trimmed UTF16 length3, legal identifiers trimmed nonblank <=64; exact retained strings are not normalized. Box finite coordinates, nonnegative width/height. Resolution values are exact terminal/provider_failure constructors, not arbitrary provider output. Read producer validates hashes, actor and provenance chains before projecting.

Native generic bank/options SDK request discards ALL non2xx bodies including422; writes5xx/chunk/malformed/oversize become unknown, read chunk transport, read malformed/oversize protocol. Native extraction custom SDK retains code-bearing business false; known provider-disabled503 is narrow known refusal, all other5xx/408 unknown for write. Original extraction retains curated200/422 domain errors and exact provider-unavailable503, not arbitrary authority bodies. Original party retains only curated422 domain codes and strict200 projections; write5xx/408/malformed/chunk/oversize unknown. Original expense retains valid snapshot/book/refusal200/422, write5xx/408 unknown; invalid422 is server_refused. These transports are not standardized; absence or rejection of a result is not rollback. Responses bounded16MiB. Explicit protocol discovery precedes sends, no ambient proxy/redirect/automatic retry.

## Atomicity and recovery

Party plans are business-read-only; apply/internal/supersede append metadata events, not posting. Current backup/source/hash/role/principal authority precedes key lookup. PlanHash covers source SHA256, metadata payload hash, party snapshot/role and exact evidence. Party keys are company event-table namespaces, not the expense receipt engine; no TTL or pruning. Same current decision may naturally deduplicate without key; historical superseded decisions cannot be silently reactivated. Actor/principal equality assertions never grant authority.

Extraction prepare can commit started attempt and service precharge, then release SQL and Owner before provider I/O. Finish reacquires and verifies original PDF bytes/hash, actor/company/ledger/context and current authority. Provider call or metadata evidence can survive later refusal. Read retained state; null/started is not proof no provider was contacted. Provider version/context/source cache is not caller retry permission. No inferred document metadata, posting or provider approval.

VAT preflight read never contacts provider; explicit apply may commit requested evidence, release locks for VIES, then retain safe result/preflight pass/fail and exception on finish. Current source/supplier/extraction and identity rechecked. Cache freshness is evidence reuse, not replay. Original CLI defaults read; --apply yes only mutation; MCP separates read and apply.

Expense booking atomically posts journal/document association/bank reconciliation in company SQL. Source/period/account role/backup/transfer fences and posting horizon remain. Keyed original HTTP/MCP uses ExpenseBook ledger UUID + operation + stable principal kind/subject + caller key hash scope, canonical validated payload,30-day outcome and permanent tombstone; expired key is not new intent. Diagnostic refusals may commit audit only. Original CLI intentionally unkeyed. Reconciliation never means replay, missing response never proves noncommit. File publication is not involved in these retained-document operations and must not be conflated with SQL commit.

## Validation scope

Only source reading and existing strict Python oracle are permitted here. No Cargo, server/browser, provider, financial, posting, upload, scanner, consent/job or authentication effects. Existing accepted runtime/userflow evidence remains at its actual scope without replay. Full #677 every-advertised-operation/all55/J01–J07/final manual desktop/release/cutover/retirement gates remain open to parent acceptance. The runner captures the artifact commit with Work-Issue:#677; worker does not commit/push/merge.

### Source-review precision

The post-preprocessor CLI flag schemas are not raw argv arrays. `original_args` deliberately lets --json win and otherwise the last --format win (duplicate format selectors are consumed there, not falsely rejected by the later leaf). Native main output is compact JSON on stdout; native usage/client errors also JSON stdout except earlier structural diagnostics. Direct native party typed structs allow nullable Option strings and complete declaration-order sequences; source-bound business validation (safe IDs, roles, hashes, nonblank reasons, confirmation) follows syntax parsing. Original party HTTP and all MCP paths remain object-only. Booking-options validates Metadata before publishing its projected purchaseVatLines, so the operation schema constrains that field to null or the typed bounded VAT-line array rather than copying a generic historical JSON reader assumption.

Validation receipt: `python3 /tmp/r51-677-oracle.py voucher_party_extraction_expense` passed the frozen52 coverage/sourcehash/strict-schema check. `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"` completed successfully into worker build observations (34003 nodes/107665 edges at extraction). These are source/structure receipts only, not semantic acceptance or live-provider/runtime proof. No commit was made by the worker; the runner owns capture with Work-Issue:#677.

Lead semantic correction: original Confirm sequence length1..2 follows omitted-default/customString semantics. Native CLI retained code-bearing false results use a distinct nested refusal envelope. Positive i64 raw HTTP event IDs remain separate from JS-safe original SDK projections; document input limits are unchanged. Framing RPC errors have null IDs; Session request IDs are bounded UTF-8 strings or i64/u64. Only selected reachable RPC parser messages are advertised. Native tuple declaration order and scalar original SDK projections verified in final worker candidate are retained. No runtime/provider/finance effects were executed.

## 2026-10-25 — Shared owner purchase VAT preflight binding

The existing bill-booking form now chooses the original Rust preflight route
from the verified native backend profile. The native GET/POST result is a flat
14-field source snapshot, including meaningful HTTP200 ok:false states. A
scoped, source-validating transport opt-in permits only that checked 2xx domain
result. Default transport, auth/non2xx refusals, protocol admission and the
legacy Bun route/envelope remain. No server route or accounting engine changed.

The UI shows required evidence, failures, cache expiry and exception action.
It freezes bill/supplier/company/auth/options context before explicit provider
confirmation. The existing ConfirmDialog owns one canonical persisted barrier
shared with the form; reads/remounts cannot release it, and explicit human
reconciliation sends no POST. The existing Rust purchase writer is retained.

75 distinct targeted cases and the proper app-directory design/TypeScript/
bundler build passed. A lead-added case checks the critical accepted-but-lost
reply scenario: even a valid canonical GET after remount must keep booking
blocked until explicit reconciliation. Production code did not change during
that test repair.

The current actual browser reached normal two-user MFA, inert read/cancel,
a genuine unavailable-provider result with durable exception, a new explicit
validation obtaining real synthetic evidence, and one bill/bank-linked EU
purchase journal. Independent read-only ledger inspection confirms exactly
2documents,1journal,4lines,1reconciliation,6VATevents and3exceptions. The
allocation is expense125/bank125 plus input/reverse-chargeVAT31.25. Original
bytes and the two provider responses are retained. These synthetic observations
are not live provider or tax-eligibility approval.

The larger current macro is FAILED, not a full actual PASS. Its first attempt
matched two headings before any provider/financial effects; the precise main
heading was reproduced independently. Its second attempt expected an inner
reader-denial heading where the outer route already fences the reader. The
previously committed purchase is retained. At two setup failures the lead
stopped the growing full setup: no third full run and no financial replay.
Fresh provider-loss/reader/restart/restore assertions in that macro were not
executed and remain explicit whole718 gaps. The original168 purchase-VAT
runtime/signed-restore proof and currentR102 shared-barrier actual are reused
only with their relevant source paths verified unchanged. No fresh complete
macro, installed-desktop or live-use acceptance is claimed.

All1609 existing wire contracts remain unchanged except current source pins.
Whole718 still requires its complete original screen/file/error/keyboard/
local-hosted/J03/J04 evidence. The existing R01 map has41company screens and
13static routes; mock navigation alone is not Rust capability acceptance.
The next concrete source gap is Agent-forslag: the existing React read/decision
URLs are not registered in Rust. Reuse existing exceptions semantics; approval
is an audited human decision, never automatic posting or a new agent engine.
All55 scopes and release/cutover/final manual installed desktop gates remain.

## 2026-10-25 — Preserve bound internal vouchers

A focused regression showed that the new native supplier-preflight gate blocked
existing internal_voucher booking because these documents intentionally have
no external supplier identity. The shared form now waits for source booking
options and distinguishes this existing workflow. It performs no external
supplier preflight for native internal vouchers; it requires a positive safe
source bank ID present in the current outgoing options, the same selected bank
and exempt treatment. Missing/unavailable source bank remains blocked.

The Rust writer still checks its immutable internal-voucher evidence, bound
bank row, bank date and exempt treatment. No engine, route, VAT calculation or
permission changed. A reproduced failed positive case now passes with both
missing/unavailable-bank negative controls; all37 relevant UI/API cases and
proper design/TypeScript/bundler build pass. No new full actual setup or
financial replay was performed. R105's explicitly missing fresh actuals and
all original whole718/55/release/cutover/final-desktop requirements remain.
