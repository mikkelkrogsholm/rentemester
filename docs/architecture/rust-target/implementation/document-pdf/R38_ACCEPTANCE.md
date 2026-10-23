# R38 — original #691 acceptance matrix

Work-Issue: #691. Source reviewed: `2a2ebfc223711d94338fcdb4ac3a4b2184fb3263`
(`2a2ebfc2`). This is a source/retained-evidence qualification, not issue closure.
One common Rust motor/server serves the same React web/Tauri UI; manual installed
desktop acceptance remains last. No new parser, provider, framework, policy exception,
network fixture or financial/MFA/restore journey is introduced here. Checked-in data
is synthetic only. Runtime JS is not part of the native path; Bun is permitted only
as historical benchmark oracle/build test tool.

## Full original acceptance matrix

| Original obligation | Existing source / retained proof | Current qualification / missing check |
| --- | --- | --- |
| Exact nine-input benchmark bridge | `scripts/benchmark-pdf-parser.ts`; `tests/fixtures/pdf-parser/synthetic-text-pdf.ts` | Source contract verified below. Actual combined PDFium benchmark **pending main receipt**; no invented measurements or result hashes. |
| Four-worker batch, resume, stable order and failure isolation | Historical `parseRegisteredPdfBatch` in `src/core/document-pdf-parser.ts`; native `crates/rentemester-server/src/product_http/document_pdf.rs` | At reviewed base native pending loop calls `one_parse` serially. Serial native batch is **not equivalent** to old four-worker behavior. Native batch4 restoration belongs to another worker; require integrated cold/warm/ordering/concurrency receipt. |
| Verified optional PDF evidence, no implicit accounting | Group66; `crates/rentemester-application/src/document_pdf.rs` (`prepare`, completion, `read_tx`), storage `document_pdf.rs` | Retain explicit parse/status/text/pending and source binding. Reads must not mutate; parse never categorizes, approves or posts. Group66 manual expense is separate explicit review, not extraction authority. |
| Original source / identity / hash / cache integrity | Engine `document_pdf.rs` (`ParserIdentity::validate`, `ParseResult::validate`, `decode`); storage `document_pdf/tests.rs` | Legacy cache/hash tests and SQL corruption refusal are focused proof, not actual hostile-corpus proof. Warm zero-child launches on exact nine remain pending main receipt. |
| Current authority and atomic publication | Host `document_pdf/tests.rs`: `owner_mutex_and_sql_are_released_during_worker_and_current_revoke_blocks_final`, `another_selected_ledger_or_authenticated_subject_cannot_adopt_worker_output`, `result_pages_attempt_and_audit_roll_back_together_on_storage_failure` | Preserve unlocked owner/SQL during child work and final current actor/company/source revalidation. Capability is never a grant. Concurrent canonical winner reuse must not duplicate results/effects. No new focused Rust test run claimed here. |
| Scanner / restart / source-free recovery | Group65 and Group66 registry entries below | Retained at original scope only; no replay. Scanner-off workspace-inbox behavior is not silently broadened. |
| Invoice extraction | Group167 registry entry below; `scripts/rust/run-invoice-extraction.py`; `crates/rentemester-host/src/invoice_extraction.rs` | Retain finite original 2 CLI + 2 MCP extraction journey and current service-subject/key/scope authority. No external production provider or automatic upload extraction claim. |
| Native invoice/report rendering | Group78 and Group63 below; `crates/rentemester-engine/src/invoice_pdf.rs` (`build`, `render`); `crates/rentemester-engine/src/archive_reports.rs` (`render`) | Existing immutable invoice PDF and source-aware report export proof is separate from PDF text parsing. No pixel/layout equivalence or installed desktop claim. |
| Bounded offline child and honest platform limits | `crates/rentemester-pdf-worker/src/main.rs`; `crates/rentemester-runtime/src/document_pdf.rs` (`Config`, `execute`, `Running::stop`, `decode`); artifact pins `scripts/rust/pdf-worker/artifacts.json` | Implemented resource envelope below. Ordinary corpus success does **not** qualify hostile peak memory or exact macOS 512 MiB hard cap. Linux enforcement source is not a new Linux runtime receipt. |
| Errors, unknown outcomes and original evidence | Host tests `worker_failure_and_source_failure_commit_only_attempts_and_terminal_never_duplicates`, `source_changed_after_worker_is_tamper_and_cannot_publish_or_hide_in_read`; server unknown-outcome mapping | Preserve cacheable malformed vs operational failure distinction, no false result on failure, no automatic retry of unknown effects. No broader release/cutover/desktop completion. |

## Exact historical benchmark contract (not a replacement corpus)

`documentCount = 4 * 2 + 1`. Use **exact bytes** from `syntheticTextPdf()`,
appending ASCII `% benchmark-N\n` for N = **1, 2, 3, 5, 6, 7, 8, 9**.
Input **N4** is exactly `%PDF-1.4\nthis is deliberately malformed\n`.
Do not substitute Group66's different malformed string or the supervisor's tiny fixture.
Generator retains binary header, Helvetica, xref offsets, 612×792 page and two text
lines: `Synthetic parser evidence` and `Invoice SYN-001`.

Required integrated observations: resume cursors **4/8/null**, original request and
output order, **8 successes + 1 cacheable malformed_pdf**, pending empty after cold
completion, **cold 9 launches / warm 0 launches / max 4 active** and zero active at
completion. Count actual children, not rows or elapsed time. Verify all warm entries
cached, including malformed. Report measured throughput/provenance honestly; do not
invent timing thresholds or infer parallelism from a fast serial run. The historical
script's sleep-only scheduler comparison is not native parser throughput evidence.
Its historical timing assertion is not a newly adopted native acceptance threshold.

Read-only SHA-256 of reviewed sources:

- Benchmark: `d418adf201b5b237a3934778c5fac60f7303f5b683c6a1d3c16380517ab583b7`.
- Generator: `368a0a48b59d97c951347676a18906df4b0a3ceaa50f09d7947a474b7aefe0ca`.

## Semantic bridge: PDF.js is not PDFium

| Dimension | Original PDF.js | Native pinned PDFium |
| --- | --- | --- |
| Parser identity | `pdfjs-dist@6.2.108`, `document-pdf-text-v1` | `pdfium`, `151.0.7881.0+binding0.9.4.adapter1.<library SHA prefix>`, `document-pdf-text-pdfium-v1`; runtime verifies archive/library/binding/adapter provenance |
| Layout segmentation | Directional text **runs** from `getTextContent`, NFC per run | One Unicode **character** per loose box; generated CR/LF contribute text, not physical glyph boxes |
| Direction and EOL | `dir`, `hasEol` retained from `hasEOL` | `dir` and `hasEol` **absent**, not fabricated false/empty equivalents |
| Font names | PDF.js `fontName` internal identifiers | PDFium `font_name()`; not the same font naming namespace |
| Coordinates | Rounded run transform[4]/[5] origin and run width/height; viewport dimensions/rotation | Rounded loose bounds left/**bottom**, width/height in page coordinates; `pdfium-character-loose-box-bottom-left-v1`. Not baseline/run geometry or guaranteed rotated viewport equivalence |
| Text | Run text + explicit EOL, CR/LF normalization and NFC | Character stream, CR/LF normalization and NFC; exact line endings, whitespace and segmentation need actual comparison, not substring-only equivalence |
| Bounds | 25k runs/page, 200k runs total | 25k characters/page check includes generated separators; total check combines prior emitted boxes with current characters. Character-count limits are stricter than run limits for ordinary multi-character runs; not the same accepted corpus |
| Evidence hashes | Historical canonical result/page/layout hashes | Native result and layout hashes are distinct semantic evidence; never silently reuse PDF.js hashes or relabel retained legacy evidence |

Source anchors: `src/core/document-pdf-parser-worker.ts` (`getTextContent`, layout,
text construction); native worker `parse`; engine optional layout fields and both
contract constants; storage `document_pdf/tests.rs`:
`actual_legacy_hashes_cache_no_text_pending_and_public_pagination`,
`canonical_js_edges_keep_negative_zero_exponents_and_utf16_key_order`,
`history_corruption_refuses_cache_reads_and_backup_without_repair`.
Unpaired UTF-16 surrogates are explicitly refused without rewriting retained bytes.
Same input SHA-256 does not imply same parser identity, resultHash or layoutHash.
The original nine byte pairs and exact text/count/hash comparison are pending the
combined bridge receipt; no fabricated per-character measurements are recorded here.

## Resource envelope and qualification boundary

| Limit / behavior | Implementation and qualification |
| --- | --- |
| Input | 10 MiB; worker bounded stdin and runtime input gate |
| Pages | 200 |
| Items | 25,000/page; 200,000 total, with character-vs-run caveat above |
| Text | 5 Mi UTF-16 code units (5 × 1024 × 1024), not UTF-8 bytes |
| Child wall / CPU | 15 seconds wall / 15 seconds CPU; worker deadline and CPU rlimit; supervisor wall check includes admission/hash elapsed time |
| Linux memory | **512 MiB RLIMIT_AS hard address-space limit implemented**, plus RSS polling; AS is not RSS. No new Linux hostile-runtime qualification here |
| macOS memory | **512 MiB sampled RSS every 10 ms; overshoot possible; no hard macOS AS cap**. Provenance says `macMemoryEnforcementQualified:false`; do not claim exact hard peak enforcement |
| Child stdout / stderr | 16 MiB / 64 KiB bounded pipes; curated error codes only, no child diagnostics or paths exposed |
| Termination | Dedicated child process group, group kill + child kill + wait/reap on cleanup; no leaked descendants/pipes accepted |
| Concurrency | Required global maximum **4 actual children**; serial reviewed base does not demonstrate four active. Restoration/integrated max-active and permit-release proof pending |
| Bundle / environment | Operator-admitted `Config`; executable/library hashes rechecked per launch; cleared environment, fixed cwd, pinned library only, no runtime JS/network lookup |
| Build guard | Preserve **30 GiB** preflight / **15 GiB critical** on source and build volumes; no guard bypass or shared worker target |

Supervisor focused anchors: `actual_child_deadline_overflow_crash_and_memory_are_curated`,
`strict_provenance_and_actual_input_hash_are_required`,
`admitted_executable_is_rechecked_before_launch`. These are not hostile PDF peak-memory
qualification. At 15 minutes of growing setup or two setup failures, isolate/reuse
valid proof and report the blocker; no blind retry or new infrastructure.

## Retained receipt registry — original scope, not new executions

Registry source: `docs/architecture/rust-target/implementation/progress.json`, SHA-256
`2e698f53aa66790dab9e66e95c2ac034bff0551ce25246dfb504ca2ac4ac2784`.
The following paths/hashes are **recorded there**. Read-only checkout search found
zero copies of each raw report. External `/tmp` receipts are outside this worker's
permitted filesystem; availability/content/hash re-read of those raw reports is
**not verified here**. This is not a claim they are globally missing. Parent must
verify available originals read-only; historical recorded hashes are not current
binary hashes. No new whole-#691 PASS follows from this registry.

| Evidence key / source revision | Recorded raw report (under `/tmp/rentemester-rust-reports/`) | Recorded SHA-256 |
| --- | --- | --- |
| `platform65_document_scanner` / `900baad2d8d597390dfc388f225ee2a518605a92` | `document-scanner-integrated65-space2.json` | `f035f3f1793acfea499e2378db951972493787ddea222b2e04a5a59c13a67209` |
| `platform66_document_pdf` / `21d1c245d0e7762d519c355e7c9108781666db8d` | `document-pdf-integrated66-tamper2.json` | `0f5f2dc106a7f6c66542ca54bfe0482c03aec8b9e940c346aaa5d38f84716fcc` |
| `original_invoice_extraction167` / `7a2e4db8f208eb7fd0ca2522cfc195a9a935dae4` | `original-invoice-extraction167-actual.json` | `037cfab9484427d7577c1dff120edcce4da8da22beef427d9995afb36849b4b0` |
| `platform63_archive_reports` / `3bb5ec82e6d51d725f9f7e069c830333fdaf5ce0` | `archive-reports-integrated63-status3.json` | `1e888645133cd24a856f4cc1db2becf05b0fdb897d802044b55886b435ff5029` |
| `platform78_private_sales` / `0fa5423676d354c5716895b797be790cdf15f57e` | `private-sales-integrated78-run2.json` | `7f56a1df8ba7de951a6a263009833743e2b14e3917cabb7cfd2a9733da0f36e3` |

- **Group65 scanner:** ordinary document ingest/HTTP/CLI/MCP; 10 provider requests,
  one duplicate request; current public revocation during scan; source-free restore
  and target reopen; no installed-native qualification. Source anchor:
  `crates/rentemester-runtime/src/document_scanner.rs` and server `document_scanner.rs`.
- **Group66 PDF:** three synthetic PDFs, explicit PDFium text/612×792/character geometry,
  cache/status/text/pending, reviewer 403 without attempt, separate reviewed expense.
  Physical tamper reads 409 `PDF_EVIDENCE_TAMPERED`; explicit parse records committed
  HTTP200/ok:false/source_unavailable with one failed attempt and NULL result, no
  new result/book effects. Fresh MFA restart and four signed source-free restore/reopen
  snapshots preserve originals/results/pages/attempts/journal/audit prefix without
  worker config/reparse. **Actual zero-child cache count and actual SQL tamper were
  not claimed**; focused tests cover SQL tamper/rollback/current revocation.
- **Group167 extraction:** 29 targeted tests recorded; five provider calls; five
  documents/attempts, two results, 30 fields, six resolutions, three exceptions,
  15 audits, zero journals/lines; normal MFA restart, signed source-free fresh-owner
  restore/reopen; actual service credential revoked during provider wait. Historical
  creator attribution is not inherited authority. Synthetic configured TLS provider only.
- **Rendering:** Group63 records five UI/eight CSV/PDF export checks, current rights
  and physical-source refusals, unchanged company audits, independently verified
  signed source-free restore and restart. Group78 records private/reduced invoice
  preview/PDF parity across four phases and preserved immutable originals through
  signed source-free restore/reopen; DKK actual, foreign edge cases focused only.
  Native renderer evidence does not prove parser semantic equivalence or full desktop.

## Semantic Graphify fragment and current check status

Owned-document semantic fragment (session-authored, existing session model route;
no delegated/new model route; token usage/cost **unknown**, not reported as zero):

```json
{"nodes":[{"id":"r38.acceptance691","source":"docs/architecture/rust-target/implementation/document-pdf/R38_ACCEPTANCE.md","kind":"qualification","status":"combined_benchmark_pending"},{"id":"r38.original9","kind":"contract","resume":[4,8,null],"coldLaunches":9,"warmLaunches":0,"maxActive":4},{"id":"r38.pdfium_semantics","kind":"boundary","layout":"character-loose-box-bottom-left","legacyHashesReusable":false},{"id":"r38.memory","kind":"boundary","linux":"hard RLIMIT_AS 512MiB implemented","macos":"sampled RSS 512MiB/10ms; overshoot possible"}],"edges":[{"from":"r38.acceptance691","to":"r38.original9","relation":"requires_actual_receipt"},{"from":"r38.acceptance691","to":"r38.pdfium_semantics","relation":"qualifies_difference"},{"from":"r38.acceptance691","to":"r38.memory","relation":"limits_claim"}]}
```

Graphify query ran before source investigation. Required code-only extraction ran
successfully to `$SGD_BUILD_DIR/graphify` (33,629 nodes / 106,624 edges); log:
`$SGD_BUILD_DIR/graphify-extract.log`. Code-only extraction does not ingest Markdown
semantics. No installed skill text was available in the checkout; the fragment above
is supplied for the lead's full skill/integrated semantic update, **not** a claim the
shared graph was updated. Shared graph/model/workflow files remain untouched.

Current worker checks are read-only source/registry SHA-256 and raw-report-copy
availability inspection, plus Python stdlib meaningful document/source/receipt-anchor
assertions (receipt in `$SGD_BUILD_DIR/r38-doc-check.json`). No native build, historical
benchmark execution, new runtime journey or whole issue acceptance claimed. Remaining:
main's exact nine-input PDFium combined benchmark, integrated batch4 receipts, accessible
raw receipt hash verification, and lead semantic graph integration. All prior gates,
unknown outcomes and original acceptance obligations remain binding.
