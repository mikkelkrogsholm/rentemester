# Delivery and recurring MCP exact contracts — Work-Issue: #677

This partition owns exactly the 25 selected MCP identities, not all81 selected
cross-surface identities or the whole #677 catalogue. All55, J01–J07, final manual
installed-desktop acceptance, release, cutover and retirement remain in the parent
scope. Main owns acceptance. Existing #692/#709/#711/R44 evidence and the accepted
17765f15 sales HEAD / 169606f1 recurring HEAD safety work and ten HEAD fixture
RED/GREEN evidence are retained at their actual scope, without replay.

## Artifact interpretation

The JSONL contains eight contracts for each selected identity. The draft2020-12
schema file separates advertised arguments, actual parser arguments, SDK response
tolerance, upstream native producers and complete MCP replies. Provenance hashes
bind current source bytes and literal needles. Only owned or already accepted
identity/invoice schema references are used. Discovery schemas are not producer
proof. Text content is exactly one compact JSON serialization of structuredContent;
JSON Schema cannot assert equality between that text and its sibling object.
Native tools wrap the *entire* HTTP value inside data; original create/generate/list
remove core ok/errors/appliedRules, lift nonempty rules and expose remaining fields
as data. Workspace success preserves core ok/errors inside data; partial failures
remove those two fields from data. There is no standardized common envelope.

## Entry point and parser boundaries

* Session parses JSON to Value, recursively collapsing duplicate names last-wins.
  tools::call explicitly requires an arguments OBJECT before every selected leaf.
  No public positional-array arguments are admitted by these tools.
* Original recurring dispatch precedes the global year/account type guard. Native
  selected tools follow that guard, so malformed year/account can produce the
  earlier `report selectors must be strings` error; strings then fail the local
  deny_unknown_fields parser. No ignored-argument promise is made.
* Native writes accept raw JSON body strings, enforce UTF8 byte bounds, validate
  with the SDK and forward unchanged. SDK body validation enforces structural
  types/enum/const/required/bounds, not every JSON Schema keyword. Explicit operation
  code checks hashes/IDs/recipient/distinct selections. Serde at HTTP can reject
  recognized duplicate fields that SDK Value validation collapsed.
* Original tools instead validate structured arguments after inserting confirm:true,
  then dispatch only when the actual supplied confirmation was true. Missing/false
  confirmation is a source-envelope tool refusal, after schema validation; provided
  nonboolean/null is an RPC error. Original recurring HTTP uses derived Serde,
  NOT the other invoice lifecycle's recursive Unique parser. Unknown/null invoice
  fields remain rejected by the original validator and current source rules.
* Direct HTTP derived structs may accept positional sequences with every declared
  field position (including null for Option), exact tuple length. This does not
  override the earlier MCP object guard or SDK body object requirement. The native
  HTTP sequence orders are recorded in parseProtocol, not advertised as MCP input.
* Runtime configuration/private cookie or service-key files and an admitted server
  are prerequisites; no example supplies credentials, grants or installation state.
  Private Unix local-profile transport has exact logical origin and no TCP fallback.

## Authority, effects and reconciliation

Manual native recurring HTTP is human-cookie-only. Original company recurring can
use its explicitly supported service path; service usage metadata may be committed
even for reads/refusals. Scheduled and hosted tools are human-cookie-only. A workspace
run additionally requires current workspace owner and exact configured workspace/root
assertion, not arbitrary path access. All retain current actor, immutable source,
role, closed-period issuance, backup and transfer fences. Plan hashes are current
preconditions, not client-computed authorization, revision tokens or replay keys.

Create has no replay guarantee. Generate deduplicates template/period only while
active, verifies the retained original, and never posts/sends. Retired templates
refuse even an existing period. Catch-up only queues missed periods; skip audits
resolved periods. App dispatch requires exact current owner and eligible plan,
creates immutable invoice/PDF unposted and unsent, and persists failed/unresolved
attempt evidence. Hosted consent binds the configured executor and live authority
anchors; it is not a worker start or dispatch. Revocation pauses/deactivates that
binding. Current human/service revocation and principal/credential generation changes
cannot authorize fresh dispatch; retained in-flight unknown evidence is not erased.

**Source/advertisement discrepancy:** `reconsent_scheduled_recurring` advertises
“never activates”. Actual storage appends a V2 `enabled` event from restored paused
state; replay sets state enabled, binds the reviewing actor as owner and clears the
required restore-consent flag. It does not dispatch or start a timer/job. Contracts
record this actual transition rather than repeating the misleading description.

Email admission reserves durable immutable MIME/source identity, rechecks current
fresh human or live service authority before one SMTP handoff outside transactions,
then records terminal recovery evidence. Final250 means SMTP acceptance, not inbox
receipt. Definite refusal and unknown are distinct. Known409 delivery_unknown and
422 delivery_refused retain canonical delivery under the native MCP data envelope;
other SDK failures use error.code/message/status instead. Status is not SMTP and
verifies source bytes. Reconcile requires external evidence/current stateHash and
never sends. Explicit reviewed retry requires definite nonacceptance, unchanged
source/sender and current authority; it is never an automatic resend. No selected
tool supports a caller idempotency key, TTL or tombstone promise. Absence under
changed source, year, ledger identity or access is not noncommit proof.

Workspace execution is a separate effectful flow: per-company generation plus
configured email **dry-run only**, or configured public delivery/status adapter.
Accepted-pending observation never falls back to send. Partial failures/continuation
counts remain data; they are not blanket retry authorization or live-provider approval.

## HTTP HEAD scope

MCP uses GET/POST only. Do not infer HEAD behavior by copying GET constructors:
manual/original recurring lists return early405 with Allow:GET before credentials or
body parsing; scheduled HEAD path returns404 before Owner dispatch; hosted HEAD405;
email HEAD is a status read, not SMTP, and supported service admission may write usage.
Wire HEAD body is zero, including middleware errors; a JSON constructor schema is
not a HEAD wire-body schema. No live ledger HEAD test was performed.

## Verification and boundaries

Graphify query ran first. Native source was traced through session/global guard,
advertisement, parser/dispatch, SDK transport/body checks, HTTP handlers, Owner,
application authority/transaction fence, storage projection and recovery paths.
Independent `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`
completed successfully. No Cargo, server, browser, network, SMTP/IMAP/provider call,
financial/mail/consent/job/auth effect, framework or dependency change was performed.
The existing strict oracle is an artifact check, not semantic/runtime acceptance;
main independently reviews actual source and diff. No commit, push, merge or issue
status change is performed by this worker.

Artifact validation: `python3 /tmp/r49-677-oracle.py delivery_recurring_mcp`
passed for exactly25 source-bound operation rows and resolved schema contracts.
This is not a full #677 acceptance or live-provider/installed-desktop test.

Lead source review: Specification.payload preserves source-bound historical stored Value rather than restricting old data to current constructors. Mail send/retry success binds accepted state only; reconciliation success binds accepted/refused. The already exact 409/422 refusal union and outer public object guard are retained. No runtime effects or new model attempt.
