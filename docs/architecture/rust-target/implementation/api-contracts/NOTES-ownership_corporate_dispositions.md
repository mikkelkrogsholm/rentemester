# Ownership, corporate records and group dispositions — existing #677

## Delivery and verification boundary

This partition specifies the frozen **60 selected identities**: 20 HTTP, 20 native remote CLI and 20 native MCP operations. Each row is `status: specified`, `unresolved: []`, with all eight contract sections. Schemas use Draft 2020-12 `$defs`; references are partition-local or to previously accepted partitions. This does not change the accepted 1,051 operations/30 partitions, the full 55-operation-family scope, J01–J07, desktop/release/cutover/retirement gates, or issue status. Main owns semantic review, integration and acceptance.

Evidence is static actual-source inspection, preceded by Graphify queries. No Cargo/server/browser/network/provider/financial/auth effects, uploads, scanner jobs, or prior runtime/userflow proofs were replayed. Company-specific production data is not included. No live-provider approval, legal validation or compliance acceptance is claimed.

Executed existing oracle:

```text
python3 /tmp/r57-677-oracle.py ownership_corporate_dispositions
PASS exactselected ownership_corporate_dispositions 60 resolved sourcebound schema contracts; semanticreview required; whole677 remains open
```

The oracle checks exact frozen IDs, unchanged source hashes, source needles, resolved strict schemas and complete recovery matrices; it does **not** establish semantic equivalence or runtime acceptance. AST extraction also completed with:

```text
graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"
```

Generated observations are in the independent worker build output, not these owned files.

## Wire and parser distinctions that must survive integration

* Global HTTP protocol negotiation precedes leaf handlers. Passive capabilities are not authorization. Host/origin proof and human cookie admission precede normal leaf parsing. The SDK negotiates using a separate credential-free manifest GET before each actual send, without redirects, inherited proxy or automatic retry.
* **Corporate HEAD:** all three corporate GET-backed handlers use the accepted early **405**, `Allow: GET`, before credentials/body/Owner. Axum suppresses the JSON refusal body. They are not successful GET aliases.
* **Ownership HEAD:** this handler does **not** have the corporate fence. `method != GET` enters mutation parsing, including mutation origin and JSON/content-encoding/query/body rules. After an object with `confirm:true`, the terminal `ownership`/`history` name is unsupported and returns **404 `not_found`**. Missing/malformed confirmation can return 400 first. It never reaches Owner through these HEAD routes.
* **Disposition status HEAD:** the ordinary status read handler runs, including current credentials, date query and actual two-company evidence; Axum strips the response body. Do not invent a universal HEAD405 rule.
* Corporate/ownership direct HTTP mutations require an outer JSON object; confirmation is removed before typed deserialization. Their nested ordinary Serde structs can accept full declaration-order positional sequences. The internally tagged ownership endpoint remains an object. Disposition plan accepts a full positional outer `Plan` sequence, but lifecycle writes are object-gated by the preceding `body["confirm"]` test. Disposition HTTP recursively rejects duplicate JSON keys; the other two handlers parse an ordinary JSON Map first.
* MCP `tools::call` rejects non-object arguments **before** every selected leaf; nested native-body grammar is not a public tuple-arguments API. The global year/account type guard can refuse before selected native parsing. Runtime argument structs and advertised tool schemas are recorded separately: Option selectors can accept null at runtime, while discovery advertises scalar selectors; runtime false confirmation reaches SDK refusal, while discovery uses `const:true`.
* Session IDs are strings bounded to 256 **UTF-8 bytes**, or signed i64/unsigned u64. Framing errors use -32700 with `id:null`; other code/message pairs are enumerated separately, including the selected leaf messages. Notifications never execute tools or emit replies. Native MCP tool failures remain JSON-RPC results with `isError:true`; argument failures remain -32602.
* CLI main runs original argument preprocessing and then consumes global split/attached `--local-profile`. Native `remote` argv remains opaque to the original dialect normalizer: ordinary remote value flags are split-only, and no `--json`/human-output alias is invented. A private local profile supplies normalized server/allow flags and binds an exact logical origin/private Unix socket without discovery, process launch or TCP fallback. Earlier remote service/report/source/budget/audit/backup/archive guards remain effective; recognized generic flags are not necessarily accepted by a selected leaf.
* Native remote outputs are compact JSON plus newline on stdout, empty stderr on these paths, exit0 for `ok:true`, exit1 for business/transport/refusal/uncertainty and exit2 for usage/invalid input. Native disposition unknown output retains its direct `code`, nullable recovery ID and errors, rather than an invented SDK `error` envelope. Startup failures and CLI diagnostics are not public MCP RPC errors.

## SDK schemas are not full JSON Schema execution

`domain::validate` enforces types, explicit property membership, required fields, byte-counted string bounds, array bounds and numeric bounds/enum/const. It ignores `oneOf`, `uniqueItems` and `pattern`. Ownership/corporate preflight expands source text maxima fourfold to avoid confusing UTF-8 byte counts with the server's UTF-16 bounds; the server still owns normalization and semantic bounds. Proposal facts and corporate original/links are extracted and individually preflighted before the residual body check. The original bytes are forwarded unchanged.

In particular, discovery suggests exact economic/endpoint alternatives, but actual SDK validation alone does not enforce them. Native `Disposition.disposition_id` is a default **String**: omission/empty can generate a server ID, while explicit null is rejected, despite the SDK discovery schema accepting null. Native tuple order follows Rust declaration order, including all Option slots; it is not JSON-schema property order.

The SDK reads successful JSON as permissive `Value`; producer-specific response schemas in this partition do not claim the client validates those shapes. No arbitrary top-level `data` escape hatch replaces a native producer schema. Two genuinely broader stored fields have field-specific arbitrary-JSON evidence:

1. Corporate `record.type` is read from the first stored event payload's `type` member without a typed enum check. Current ingest emits a supported string, but the stored reader itself accepts broader JSON.
2. Ownership `snapshot.diff` is hash/canonical-byte checked as stored JSON; only `conflicts` is decoded as `Vec<Fact>` on load. Current proposal constructs the exact `Diff` schema; historical readback is not falsely narrowed to that constructor. Apply independently decodes additions/endings and checks current integrity.

## Whole-flow recovery and authority

### Ownership source proposal → independent review → exact apply → readback

The anchor company is not a universal authority grant. Current endpoint-company ownership permission and readable party/corporate sources are checked for the complete incoming facts, conflicting prior observations and retained snapshot facts. Proposal needs CompanyOwnershipManage; review/apply need CompanyAdmin; read/history need CompanyOwnershipRead. Current session is used; no fresh-ten-minute requirement is invented. The snapshot's authenticated user principal cannot review it.

Proposal source/observed time and canonical-hash-sorted facts produce immutable snapshot/diff hashes. Exact ID replay only succeeds when both hashes still match the recomputed current diff. Review of the same decision is a readback only after independent-principal validation; another reviewed decision refuses. Apply requires approved state and exact hashes, then recomputes planned endings against current approved facts. Already-applied exact hashes return `status:unchanged`. Omission is never revocation; only explicit later facts can end prior facts. Graph integrity keeps nonoverlap, no self/cycle and direct economic floors within 10,000 basis points. v1 consolidation manifest authority is distinct from this flow.

Writes are one canonical control transaction. Backup checks use protected affected-company handles. Host selection-restoration failure may replace an already committed result with OutcomeUnknown. Reconcile history by retained snapshot ID and source hashes, then graph at explicit asOf; hidden/partial results and changed authority are not proof of rollback. History filters by readable complete facts; optional projection appears only when the graph is nonpartial. Projection eligibility is advisory, not v1 manifest application.

### Corporate original → immutable ingest → explicit metadata → source reconciliation

Original is **JSON `{ok,bytesBase64,sha256}`**, not a file/binary stream and not a MIME/filename or client-verification claim. Native storage keeps the actual bytes in the canonical control database and verifies size/nonempty/hash on download. Ingest is 24MiB JSON, decoded original at most16MiB; other native writes32KiB. Named MCP corporate-ingest frame is48MiB+64KiB; all other ordinary selected frames1MiB except ownership proposal4MiB+64KiB.

Same explicit record ID and source hash returns existing metadata after input validation; it does not append newly supplied facts/links. Different bytes at the same ID refuse; duplicate bytes under another ID require explicit linking. Metadata events deduplicate by record/event/payload hash; that is not a caller retry-key receipt. Enrichment is an explicit factual assertion, not a provider/legal validation. Supersession appends evidence while retaining original bytes.

Every associated company must remain readable; writes require current all-scope MasterData. Non-normal sensitivity adds an all-scope MasterData gate for detail/original. **List uses the read-visibility gate only**, not the detail sensitivity gate. Visibility is evaluated before pagination/count. Supersede replacement requires current write authority, but backup checks cover old scopes plus anchor, not an unrelated replacement-source company solely because it supplies evidence. Reconcile record/history/links and original independently after uncertainty; missing output or an unavailable generated ID is not rollback.

### Group disposition evidence lifecycle → current two-company status

Plan/propose normalize IDs/text/references and bind a canonical payload hash. Amount is a JSON **number**, not a decimal string or null; source semantics require a positive exact two-decimal value. Stable normalized references are sorted in UTF-16 order. Plan can generate an ID without persisting a disposition; adopt the returned normalized ID/body before explicit proposal, or a lost server-generated proposal result can defeat direct recovery.

Current authority includes both companies, primary anchor membership, party/corporate evidence, historical settlement references and replacement-disposition scope. Writes need WorkspaceManage and CompanyOwnershipManage on both companies; reads need WorkspaceRead and both CompanyOwnershipRead. Backup/transfer/namespace/source proof remain current. Independent approval compares stable principals, not actor spelling. Actual admitted journal evidence must be posted, nonreversed, date/currency/exact-side amount matching, with exact entry and ledger-head hashes. No head refresh or waiver is available.

Replay is state-specific: propose only while proposed with the same payload; approve returns approved/partly_posted/posted before the initial distinct-principal check; link only in approved/partly_posted, with same-side existing-ID replay after current link checks (not a posted-state replay); settle already settled returns existing before validating newly supplied settlement references; terminal supersede replay refuses; reopen only terminal, so repeated reopen refuses. Current authorization/source/backup admission still precedes storage replay. There is no caller key, key TTL or tombstone guarantee.

The Owner restores selection **before** disposition commit. Restore failure rolls back and can retire the connection; commit failure remains OutcomeUnknown. Source company read transactions are rolled back; lifecycle changes commit only to control. The lifecycle is evidence-only: it never posts journals, transfers money, balances companies or applies eliminations. Status `balanced` is true only for `posted` without exceptions; **settled is always balanced:false**. Optional asOf changes overdue evaluation, not historical state. Raw SQLite event IDs remain signed i64; no original-SDK safe-integer projection is imposed.

## Transport refusal and uncertain-write correlation

Ownership/corporate generic transport discards non2xx bodies, including422, and exposes server-refused status instead. Any write5xx can follow commit and becomes OutcomeUnknown. Corporate original uses its dedicated bounded24MiB JSON reader.

Disposition transport preserves structured `ok:false` with nonempty code even on422; CLI/MCP retain that nested native business core. Write408/5xx and recognized internal/unknown codes instead become OutcomeUnknown. Plan is a body-backed read; its disposition adapter maps send uncertainty to transport, not a write-replay promise. Status and canonical source reads are reconciliation, not replay. Across every family, no automatic retry is enabled and changed authority/absence is never interpreted as proof of no effects.

## R67/R68 original legal evidence and ownership CLI

Thirteen original commands now reuse the existing Rust SDK/server: seven corporate-record operations and six ownership operations. Exact flat results, input files/bytes, independent review, opaque snapshot/hash tokens, JSON/human streams and0/1/2 exits are retained in the explicit native credential profile. Local workspace is an assertion against the private runtime; no source database opens or TCP fallback. Review repaired multiline prose rejection and malformed ownership receipt success; source-backed fixtures/unknown-outcome checks and ten+seven compiled tests passed on the combined code. Originaldownload verifies canonicalbase64 and SHA256; projection never invents absent wholeworkspace data. Prior1492 businesscontracts retained, current1505=639HTTP479CLI387MCP across38partitions. Whole716/launcher/all55 and finalmanualdesktop gates remain open.

## R74/R75 original MCP adapters

The source-bound original named profiles now have exact flat argument/result/error/recovery contracts, using the same Rust server. Across both partitions28names are added,27supported; identity-only party creation advertises an explicit unavailable boundary and cannot infer a business role. Prior1519business contracts and389native tool definitions remain unchanged. The current1547operation union is641HTTP489CLI417MCP. Source receipt mismatches remain uncertain and never trigger replay. Full #717 is open for the remaining original names, identity-only creation and actual complete workflow/reconnect evidence. Existing financial, access, backup and final desktop gates are retained.
