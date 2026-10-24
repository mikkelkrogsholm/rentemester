# Public invoice, inbound receive, mailbox and imported receivables contracts

Scope: the 61 exact selected HTTP/CLI/MCP identities in this partition. These are additions to existing #677 work, not issue closure, provider acceptance, or a replacement for the parent's source/diff review. Prior accepted 1,285 operations/34 partitions, full 55 scope, J01–J07, final desktop, release, cutover and retirement remain unchanged.

## Source-first method and evidence boundary

Ran `graphify query 'public_einvoice bilagsmail'` before reading actual Rust sources. The graph surfaced historical TypeScript context; Rust handler/adaptor/application/storage/runtime bytes govern these contracts. Provenance records current SHA256 plus source needles. Only these four flat partition files are owned. No production, dependency, inventory, gap, workflow, protocol, or other partition changes; no Cargo, server, browser, provider, IMAP, SMTP, authentication, posting, scanner or upload replay.

The existing strict Python oracle checks identity coverage, source hashes and resolvable explicit schema shapes. It does not certify producer semantics or live provider approval. The parent owns independent review/integration/acceptance.

## Important transport distinctions

* MCP `tools::call` rejects non-object arguments before all selected native/original dispatch. Direct HTTP Pair/Send/receive/configuration Serde sequence grammar is not public MCP tuple grammar. MCP outer confirmation parses a boolean; false becomes an SDK error, while discovery advertises true. The exact JSON body string is independently preflighted as an object by the SDK.
* CLI `original_args::prepare_with_output`, discovery, then `local_binding` run before leaf execution. The global split/attached `--local-profile` is consumed and joins a private Unix socket at an exact logical origin, with no TCP fallback. Public argv is not the normalized leaf flag map. Native remote recognized flags can be rejected by earlier source/report/service/audit/budget guards. Original stdout/human/JSON and parse streams differ from native compact JSON.
* Native generic SDK `request` accepts any bounded JSON on HTTP success, without validating its domain producer shape. These contracts constrain the actual selected native producer, not a promise that the SDK validates all fields. Native MCP returns `{ok,data}` or `{ok:false,error}`; a returned business false can retain nested data. Original pure validation uses errors/appliedRules/data and isError. Original public transport validates artifacts and retains only complete validated 422 business receipts; generic public/mail/receive transport discards 422 bodies. Dinero read transport preserves returned `ok:false`/nonempty code receipts.
* No universal replay promise or caller retry key. Natural submission/dedup identities are not permission to retry under changed authority. Reconciliation is not replay, a missing result is not rollback, and changed credentials or permission do not erase prior effects.

## HEAD: preserve actual handlers, not a generalized fence

R54's accepted 11 handlers/18 routes remain unchanged. Historical R58 observed four selected GET-backed routes without that early fence. R59 corrects the two public invoice routes; mailbox/imported behavior remains unchanged:

| HEAD route | Actual leaf behavior after outer negotiation |
|---|---|
| bilagsmail | Mutation origin/cookie proof, selected/query/encoding checks, unmatched method returns 404 not_found before worker/Owner. |
| imported-receivables | Non-applying cookie proof, selected/query/encoding/idempotency checks, method falls through to 405 invalid_input; this constructor does not add Allow. |
| public-config | R59: unconditional 405 Allow: GET before proof/body/Owner/provider. Historical unsafe send fallthrough was corrected. |
| public-submissions | R59: same early 405 Allow: GET fence as public-config, never a submissions GET alias. |

Axum suppresses all wire response body bytes for HEAD while preserving status/headers. R58 ran no probes. R59 uses the existing synthetic real-server fixture: RED401, then GREEN405 for public routes, with 64 HEAD cases over31routes, GET/POST auth and exact accounting/artifact invariants retained; no provider actions. HEAD remains a refusal, not a GET alias.

## Family controls and recovery

### Public invoices

Registration and onboarding use fixed operator credentials and canonical local DK CVR identity; onboarding authorization is checked before registration. Pair registers outbound and inbound on one named network, retaining partial phases. `technicalReady` means the two technical directions, never external candidate approval; `externallyApproved` remains false. Separate deliberate calls discover existing registration before additional provider mutations.

Immutable issued JSON/PDF physical proof, ledger/source hashes, current authority, environment and credential generation are checked around provider phases. No Owner guard/SQL transaction spans provider I/O. Send reserves durable evidence before delivery. A queued `prepared`, accepted terminal `failed`, reserved no-ID `uncertain`, and delivered `acknowledged` can all be outer HTTP200/ok:true. Preserve transmissionId, providerStatus, stateHash/history and redeliveryBlocked. Any blocking earlier invoice handoff, including historical/legacy or changed credentials, prevents blind redelivery. Only a verified harmless metadata preparation with no attestation/events may be bypassed.

Original export-public is a local XML artifact projection; OIOUBL export additionally audits. Metadata Peppol handoff is not a provider URL fetch. Original Digisense transmission uses current native admitted dispatch and original projection. Original CLI validates SHA256 before `std::fs::write`; server commit/provider outcome is not undone if local publication fails. Inspect canonical submissions before another explicit action.

### Mailbox

IMAP password is write-only request data; status exposes only safe host/port/TLS/username/mailbox, not password. Omitted secure defaults true, omitted/blank mailbox normalizes INBOX; explicit false is plaintext IMAP, not invented STARTTLS. Deletion has no body. Alias requires an explicit nullable field: null or raw empty string clears; trimmed/lowercased nonempty aliases must meet source ASCII 3–64 grammar (whitespace-only is refused).

Config file write/remove precedes SQL audit commit: no combined file/SQL atomic rollback claim. Poll captures current grant/config epoch/cursor and authority, releases Owner during IMAP, then rechecks before retaining original EML/attachments/quarantine/dedup. Poll provider error may commit an error audit and return 422; generic SDK discards that response body. Retention and scanner controls remain. No extraction, inferred metadata, posting or sending.

### Digisense receive

Receive-only explicit polls use current CompanyDocumentsUpload, fixed bound company/environment/credential generation and per-record consumed admissions. Workspace preflights all active initialized companies for authority/backup before any provider work. Stable internal IDs and durable quarantine preserve dedup; download errors do not poison terminal dedup. Good siblings and partial company outcomes remain. Individual quarantined/error documents may coexist with batch ok:true and errors; a top-level phase error sets batch ok:false. Workspace remains outer ok:true with per-company failed/skipped states. Read authoritative documents/dedup after uncertainty; never automatically repeat a provider pass.

### Imported receivables and validation

Imported schedule reads reverify accepted physical source/archive evidence and remain distinct from native issue/post/settle. Exact raw optional `asOf=YYYY-MM-DD` uses trusted UTC today when absent. Payments are included by effective date; settled rows remain. Amounts are JSON numbers produced from exact Money, not decimal strings or native invoice operations.

`invoice_validate` is a pure local advisory Value reader, not the stricter issuance constructor. CLI accepts any parsed JSON; MCP requires an object payload. Extra/malformed evidence fields reach the validator to produce its verdict. Field-specific recursive arbitrary evidence schema is confined to that offline payload and copied invoiceType, not a generic remote data escape hatch. No company, number allocation, credentials, provider or ledger authority is conferred.

## Checks performed

`python3 /tmp/r58-677-oracle.py public_invoice_inbound_bilagsmail`: PASS, exact 61 selected identities, resolved sourcebound schemas; semantic review still required, whole #677 remains open. Two initial schema-format check failures (HEAD wrapper representation and required-only selector branches) were repaired in owned files; no test harness/setup expansion.

`graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`: completed, 34,009 nodes / 107,681 edges, generated observations confined to independent build output.

## Lead source-review repair
Lead source review repaired four original public-invoice CLI refusal JSON variants to include actual local fail() errors-only DTO and validated422 business core/domain codes alongside client errors. Human stderr differs and local publication may fail after successful server audit. No provider/effects replay. R59 removes the accurately observed unsafe publicHEAD blocker by a first-statement handler fence; actual existing-fixture RED/GREEN proof binds current source/binary/logs. Full #677 acceptance still requires all1490 exact contracts, retained protocol proofs and lead source review.

## R59 bounded correction
Two public HEAD contracts now specify early405AllowGET and bodyless409 protocol refusal. Other GET/POST contracts retain behavior; current source hashes refreshed here and in delivery_recurring_http only. Historical R58 reviews and failed receipts are preserved.

## 2026-10-24 — R87 original contact-import appendix (Work-Issue: #716)

Exactly two CLI operation identities added in discovery/setup: `import systems`
and `import contacts`. Current contract union is 1596 = 642 HTTP + 529 CLI +
425 MCP across the existing 38 partitions; historical measured source/catalogue
snapshots above remain historical, not new measurements. Existing native contact
body definitions gain only optional `defaultRole`; omission retains strict native
headers/vendor fallback. Explicit role enables original CSV headers (only
Kontaktnavn required); classification and prepare/execution/GDPR stay server-owned.
Root help adds the existing integrated import descriptor. Closed result/help
schemas preserve real summary values and successful warning errors.

Independent compiled `original_import_contacts716` test target: 5 passed, including
HTTP exact bodies and private Unix/workspace binding; no real CVR/provider/auth
setup. Malformed receipts are outcome_unknown, no automatic retry. Full catalogue
and combined acceptance remain lead-owned. No import-run accounting/archive,
opening balance, new parser/provider, rootbootstrap or whole-#716 closure claimed.

Lead review corrected direct-HTTP ImportObject/ImportHttp for actual Option<Role>: null/omitted retains strict policy, original four-position prefix remains and optional fifth customer/vendor/null is accepted. SDK remains optional string enum only. Existing engine wire tests already prove the four/five-position producer; no new financial/provider/auth effects or runtime changes.
