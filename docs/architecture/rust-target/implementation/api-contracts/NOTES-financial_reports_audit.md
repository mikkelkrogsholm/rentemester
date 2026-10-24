# Financial reports and audit — #677 scoped contract delivery

42 frozen operation rows are specified with all eight contract fields, partition-local Draft2020-12 schemas, and current source-byte provenance. This bounded repair retains the previous substantial schema/source work rather than regenerating accepted partitions. Main owns actual-diff semantic review, integration and acceptance; oracle validation is not semantic acceptance or issue closure.

## Repair and checks

- Repaired the empty operations JSONL and provenance stub with 42 incrementally persisted rows and 46 focused source bindings.
- Preserved existing definitions; added exact protocol refusal, bounded dated template selector correlation and retained export-businessfalse wrappers. Tightened successful native envelope booleans and cashflow public year typing after the global MCP selector guard.
- Existing check: `python3 /tmp/r56-677-oracle.py financial_reports_audit` — **PASS**, 42 exact selected operations, resolved source-bound schema contracts; whole677 remains open.
- One repair setup failure referenced nonexistent MCP `src/lib.rs`; corrected to actual `src/main.rs` Session declaration. No new harness, dependencies or test infrastructure.
- Graphify query ran before actual-source reading. Required extract is run before delivery, outside owned files.

## Preserved source findings

- Selected report/audit/cashflow/portfolio overview leaves have method!=GET guards after origin/credential proof (and selected company where applicable), not the R54 early405 fences belonging to other routes. Axum GET routing invokes them for HEAD with the actual HEAD method, then strips response bytes. Export `product_http::read` has no method guard and can execute the admitted export for HEAD. Do not transplant a universal405 contract.
- `reports/{report}` is bounded trial-balance/profit-loss/balance, not arbitrary report JSON. Dated reports are flat numeric original DTOs; invalid calendar/range is HTTP200 businessfalse. SDK discards non2xx dated results. Experimental local trial-balance is separate, exact Money strings and format rentemester-trial-balance/1.
- Live cockpit financial amounts are numeric, archive financial amounts exact Money strings/null; archive classification completeness controls null totals. Source selection is live first with archive fallback only when source omitted and selected year has no live match. Explicit archive has no fallback; date-selected archive refuses.
- Export binary CSV/PDF source is report_export::build; incomplete archive financial basis refuses. Exact exported bytes max96MiB in SDK integer-array wrapper. SDK export uniquely retains a non2xx JSON core if ok:false and nonempty code; native CLI retains nested data core on businessfalse.
- Global MCP tools::call rejects non-object args before original/native dispatch. year/account global check rejects explicit null before ReadArgs Option parsing; source Option accepts null. Portfolio_overview original early dispatch occurs before this report selector guard and preserves its original errors[]/code envelope.
- Audit list SQL producer ID is signed i64; actual Rust SDK checks positive i64, not safeinteger. Filter null forbidden; empty text filter means absent; limit1..5000 default500, offset safeinteger default0; count+total same transaction, pages separate observations; created_at DESC,id DESC; max8MiB with no truncation. SQL LIKE %/_ wildcards remain meaningful.
- Audit verification outer ok:true can contain audit.ok:false / unavailable components. Fixed five-component order ledger/bank/invoices/originals/physical_schema. Raw findings require component-specific exact schemas, including PRAGMA foreign_key_check row shape. SDK audit.entries narrows to safe nonnegative integer.
- Workspace portfolio overview uses current readable company set and slug order; it differs from native portfolio numeric rollup. Dashboard projection deliberately converts absent backup.requiredPermission / VAT basis / receivable reason into null. Metadata is current state; no historical state reconstruction or consolidation claim.
- Exception sourceEvidence/postingPreview genuinely read arbitrary retained JSON via exceptions::parsed; own field-specific arbitrary-JSON schema was preserved. Do not substitute unrelated accepted StoredJson scope.
- CLI local_binding consumes global local-profile split/attached first, private Unix exact-origin noTCPfallback; normalized remote flags differ public argv. Earlier remote guard rejects service-key-file for report-export/portfolio/cashflow, notwithstanding later helper support. Original dated parser accepts attached values; native remote parser split-only; local experimental parser exactly three split pairs.

## Scope and review boundaries

The first four contracts connect public parsing/output/refusal/protocol representations to retained source schemas. HTTP HEAD notes distinguish suppressed constructor JSON from actual bodyless wire responses; selected leaves are not projected into unrelated R54 early405 behavior. Dynamic authority/stage failures retain their source-bound mappings rather than claiming a clean audit on successful transport. RPC errors remain explicit code/message pairs, not a code/message Cartesian product.

No runtime, Cargo, server, browser, network, SMTP/IMAP, provider, posting, financial, upload, scanner, consent, job or auth effects were executed. No live-provider approval or universal replay key is claimed. Repeated observation is not replay; missing response and changed authority do not establish rollback of control/service metadata.

Only the four owned flat files changed. Prior accepted partitions, all55/J01–J07/final desktop/release/cutover/retirement, and accepted runtime/userflow evidence retain their actual scope without replay. No commit/push/merge or issue status change.


## Lead source-review repair
Actual service_company constructors append required usage/business on admitted results. Four native report envelopes now retain a distinct human branch and service branch (business committed; usage committed/not_committed). Failures retain human shape plus required serialized service stages; status correlation and HEAD stripping remain explicit. Existing raw Money, nested audit availability, arbitrary evidence JSON and CLI/MCP adapters are unchanged.
