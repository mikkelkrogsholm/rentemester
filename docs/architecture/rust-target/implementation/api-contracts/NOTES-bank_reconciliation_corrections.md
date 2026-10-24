# Bank reconciliation and corrections — existing #677

## Delivered scope

41 exact selected identities: 18 HTTP, 13 CLI and 10 MCP operations. All rows declare `status: specified`, `unresolved: []` and the eight required contract sections. Draft 2020-12 definitions are partition-local except references to the previously accepted `chart_journal` partition. Current-byte SHA-256/needle bindings accompany the operation callchains and provenance.

This is source-contract evidence, not runtime acceptance. Main owns independent semantic review, integration and acceptance. Prior 419 accepted operations/18 partitions are unchanged. Whole #677/all 55/J01–J07/final desktop/release/cutover/retirement scope remains retained; this deliverable closes no issue.

## Important distinctions preserved

- Global HTTP protocol admission precedes leaf credentials and parsing. Current Origin/Host, human proof or explicitly supported original service proof, selected company and held Owner admission are not replaced by capability advertisement.
- Native bank GET auto-HEAD dispatches the read handler; Axum suppresses its response body. Original bank GET auto-HEAD reaches credential/company checks and then rejects the non-GET method with `400 invalid_query`. Neither behavior is the native payables early-405 fix. No HEAD requests were made and no blanket HEAD safety is claimed.
- MCP `tools::call` rejects nonobject arguments before all selected dispatch. The overlapping `bank_suggest_matches` and `bank_reconciliation_correction_plan` names select original flat adapters, not the otherwise present native helper. Native direct HTTP typed structures retain declaration-order sequence decoding, while SDK bodies and public MCP arguments remain objects. Original custom optional text/principal fields reject explicit null but permit omitted default sequence tails.
- CLI global `--local-profile` is consumed in split or attached form before dispatch; it joins a private Unix socket with exact logical origin and no TCP fallback. Original argv accepts attached values, native remote argv does not. Original writes use `--confirm yes`; native remote writes use bare `--confirm`. Earlier remote service/report/source/budget guards may reject recognized selectors before the leaf.
- Advice retains source `ok:false` on HTTP 200; `max` is a per-bank suggestion limit (default 5), not a bank-row limit. Confidence is a ranking score and may exceed one. Original CLI advice has its actual human text renderer, not the native JSON wrapper.
- Reconciliation links and corrections append evidence only. Original links may also resolve unmatched-bank exceptions. Direct-payable correction is a different atomic company writer: exact original reversal, payable recognition, settlement on bank date and reconciliation supersession. Their receipts/effects are not shared.
- Native bank correction plan has a nested core `ok/plan/errors` inside the HTTP `plan` member. Original envelopes strip core `ok/errors` into their envelope; original CLI successful output lifts data again. Native CLI/MCP business refusals can retain a failed core under `data`, rather than always constructing an SDK error wrapper.
- Generic bank SDK transport discards non-2xx bodies. Original direct correction likewise discards 422 bodies and treats apply 5xx/408 as unknown. Native direct correction preserves well-formed code-bearing business refusals, including retained 422/409, while uncertain apply outcomes become unknown. Service usage and business stages are independent.
- Canonical plan source rows are explicit SQL-field projections, not arbitrary top-level JSON. Stored NUMERIC fields read through the generic SQL row producer can be numeric/string/null as appropriate, whereas `sales_invoice::wire` amounts are JSON numbers and serialized engine money in the bill request/line tuples is decimal text. Raw producer IDs/history events remain signed i64; safe selectors remain bounded separately.

## Recovery and retained controls

No automatic retry, client-generated hash/key, newest-match selection or provider approval is promised. Supported keyed correction operations bind ledger UUID, operation, principal kind/subject, raw key hash and exact payload hash. Retained outcomes last 30 days; tombstones survive expiry, preventing key reuse. Current authority and archive/source/original proofs still precede replay. Direct correction additionally proves the unique document/bank correction and all retained accounting relations. Original link natural deduplication validates the exact stored intent/principal/audit proof; native duplicate link is a refusal, not replay.

After uncertainty, inspect canonical bank reconciliation/history and journals; direct correction also requires payable/payment readback. Preserve the exact original body/key/principal. Missing response is not rollback; changed authority is not absence of effects. Read/plan/refusal/service usage may write metadata. Backup governance, transfer fences, accounting periods, trusted posting horizon, original document/scanner provenance and source/hash integrity remain admission controls. File publication and SQL commit are not interchangeable guarantees.

## Checks

- First action: `graphify query "bank reconciliation correction"`; followed by current source callchain verification.
- `python3 /tmp/r53-677-oracle.py bank_reconciliation_corrections` — **PASS**, exact 41 selected IDs and current frozen source hashes, resolved source-bound schema checks. Oracle explicitly retains semantic review and whole #677 openness.
- `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"` — **completed**, 3,800 code files; output outside owned code under the independent build directory (34,009 nodes, 107,681 edges).
- No Cargo/build/server/browser/network/provider/SMTP/IMAP/auth/financial/posting/upload/scanner/consent/job effects, HEAD probes or accepted-runtime proof replays were executed. No new harness/dependencies/production edits. Only the four owned flat files are delivery artifacts.
