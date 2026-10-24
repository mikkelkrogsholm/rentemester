# Financial reports and audit — #677 partial handoff

**Not complete; not accepted.** Main retains integration and semantic acceptance. No issue closure, runtime replay, provider approval, or whole-epic acceptance is claimed. Prior accepted partitions and all55/J01–J07/final desktop/release/cutover/retirement remain unchanged.

## Concrete blocker / stop rule

Stopped after two artifact-generation failures, as mandated. First generator failed with `NameError: name 'REDACTED' is not defined` while declaring export byte maxItems; subsequent incremental writes preserved schemas using 96*1024*1024. Second operation-generation attempt failed its source anchor assertion: `crates/rentemester-application/src/periods.rs`, needle `pub fn audit_evidence(` does not match the actual declaration. The generator kept rows in memory until its final write, so operations JSONL remains empty and provenance remains initial metadata. Do not interpret status metadata in these drafts as acceptance.

Existing oracle executed: `python3 /tmp/r56-677-oracle.py financial_reports_audit` **FAIL**, `AssertionError: Coverage/duplicates/extra/gaps` (empty operations file). No PASS evidence is claimed. No test setup was added.

Graphify query ran first. Required `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"` completed: 34009 nodes / 107681 edges; generated observations remain outside owned files.

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

## Draft limitations requiring lead repair

Schemas are substantial partial declarations, not semantically accepted. Operation records, sourceRefs and provenance still require generation/inspection. RPC schema includes selected leaf parse messages and session framing pairs but is not yet connected through operation rows. Success/refusal correlations and parser schemas require final review; no oracle-only semantic acceptance is implied. Do not integrate as complete.

Only the four owned flat files were written. No production, dependencies, other partitions, runtime/server/provider/financial/posting/auth effects, git commit or issue status changes.
