# Rentemester: behavior-preserving simplification

## Mandate and completion

Refactor the complete product so its code is easier to understand, change and
verify. Preserve supported behavior, external CLI/MCP/HTTP contracts, user flows,
data compatibility, accounting rules, audit history, actor/access gates and
explicit confirmation. Use Graphify to locate dependencies and source/tests to
verify them. No new features, schema/rule changes or company-specific defaults.

The bounded improvement list below comes from a Graphify-first audit of core,
imports, agent runtime, CLI/MCP/server and cockpit code and tests. The full goal
remains active until every selected item has implementation and acceptance
evidence, integrated checks pass, the graph is updated, and a final review
confirms the scope and reports limitations. Ordinary implementation decisions
remain with the lead. A simplification that requires changed product behavior
or lacks sufficient verification must be raised with the user.

## Improvement list

| ID | Change and demonstrated pressure | Acceptance | State |
| --- | --- | --- | --- |
| R1 | Share cockpit JSON/binary transport and error handling. PDF preview and accountant export duplicate the canonical seam; binary 401 does not expire the UI session, and export network failure lacks the code used to lock uncertain writes. | Failing tests before fixes; error envelopes, auth expiry, network/body failure, read/write distinction, one request, PDF/tar payloads and headers preserved. | Implemented; integrated audit pending |
| R2 | Give CLI mutation scope one explicit owner; workspace disposition settle/reopen/supersede incorrectly require a company before their own workspace preflight. Remove repeated period reopen actor resolution while preserving its historical empty-policy distinction for explicit versus derived machine actors. The spec-driven check also found three ownership scope omissions and three unclassified company-knowledge writes. | Synthetic failure reproduction; correct workspace dispatch; allowed/denied/derived actors, attribution and period lifecycle tests. | Implemented; integrated audit pending |
| R3 | Share purchase supplier/recipient evidence and 25% consistency checks used by direct-bank expenses and payable registration. Keep workflow-specific treatment inference, validation order and result rules. | Both paths: identity/context refusal, malformed payloads, rate/tolerance, mixed VAT, FX and nonregistered-company cases; preserve first error and rollback. | Implemented; integrated audit pending |
| R4 | Replace repeated transaction-rejection JSON codecs with one structured error/decoder across the existing invoice mutation paths. Retain legacy message fallback and operation-owned transactions/rule merging. | Failed journal/evidence/application writes roll back rows, audit and sequences; retry remains gap-free; invoice lifecycle/negative suites pass and result shapes are preserved. | Implemented; integrated audit pending |
| R5 | Remove five cockpit refresh counters already implemented by useAsync.reload. These counters mistakenly change resource identity and discard existing data. | Deferred same-resource reload and refresh failure retain data; real company/filter changes hide old data; five view suites and hook tests pass. | Implemented; integrated audit pending |
| R6 | Share the exact invoice input serializer between preview and issue. | Both endpoints send identical data; optional truthy/undefined omission semantics and zero VAT preserved; invoice modal tests pass. | Implemented; integrated audit pending |
| R7 | Remove unreachable legacy Dinero archive/bilag branch and private helpers; the identical earlier condition always returns via atomic v4. Keep both active preflight calls and all active v4 effects. | Import framework, archive integrity, atomic failure/dry-run/file rollback, receipt links, open items and immutable retry suites pass. | Implemented; integrated audit pending |
| R8 | Agent loop imports the CLI rendering barrel for a wrapper that merely calls core/money.formatKronerDa. Call the canonical domain formatter directly. | Runtime agent, Danish output and money formatting tests preserve outputs; agent has no CLI rendering import. | Implemented; integrated audit pending |
| R9 | Share the repeated strict read-ledger lifecycle in MCP's adaptive and explicit read-only wrappers. Preserve their public envelopes and policy differences. | Whole-tree read-only side-effect checks, schema refusal/inspection, slug resolution, redacted errors, guaranteed close and concurrent read/write isolation. | Implemented; integrated audit pending |

Execution increments: first remove the proven obsolete paths/dependencies and
repair CLI policy ownership; then consolidate cockpit behavior; then consolidate
core safety rules and transaction rejections. Each increment has focused tests
and a reviewed diff. This order does not reduce the full improvement list.

## Baseline, 2026-10-03

- Checkout: `codex/release-render-fix`, HEAD `42a91717`; Bun `1.4.0`.
- Prior Graphify upgrade/rebuild changes and dated graph artifacts are preserved.
  No product code was dirty before this mission.
- Graph: Graphify `0.9.74`, 9,086 nodes / 33,769 edges. Structural connections
  may be undirected or collapsed; proposed changes were checked against source.
- Runtime typecheck, lint (1,165 files), agent-discovery coverage, cockpit build,
  CLI smoke and MCP smoke passed before code changes.
- Cockpit: 570 tests / 67 files passed; browser: 167 tests passed.
- Backend: 2,660 passed, 1 failed / 415 files. The first bank-suggest-matches
  CLI case returned exit 1 instead of 0 under the full parallel suite. All five
  cases pass in isolation. Cause remains unproven; do not describe baseline as
  entirely green or delete/weaken the test. Final full verification must revisit.
- Focused import baseline: 83 tests / 8 files passed.
- Supply-chain audit (259 packages), production licenses (132 packages), PDF
  benchmark verification and container integration passed. Container
  reproducibility baseline also passed (session 14006 completed with exit 0).
- Discovery surface baseline: MCP 214, CLI 265, HTTP 208. Hashes are saved in
  the baseline-discovery JSON for comparison after refactoring.
- Detailed command output: `/tmp/rentemester-refactor-x57lVC/`. Temporary paths
  are local evidence, not portable product prerequisites.

## Decisions and deferred findings

- Keep direct-bank expenses and payable registration separate: their FX,
  treatment, internal-voucher and validation sequencing contracts differ.
- Keep ordinary and recurring invoice validation separate: quantity and price
  rules differ.
- Keep the outer Dinero archive preflight: removing it could change rejection
  envelopes and rejected-attempt provenance, independently of dead-code removal.
- Company-local supplier policy/account mapping in agent/contract is a broader
  product-boundary question; this refactor preserves current supported behavior.

## Implementation evidence

All nine items now have source changes. The lead owns all writes; delegated
source discovery and review were read-only. Integrated verification and the
final completion audit remain outstanding; the goal is active.

- R1/R5/R6: shared JSON/binary transport, one invoice serializer, five reload
  counters removed. Export uses the existing uncertain-write guard with stable
  company scope across both UI placements. Before-change regressions reproduced
  binary auth/error handling, refresh data loss and export replay risk. Focused
  80 tests / 12 files passed; independent frontend review found no substantive
  remaining issue. Full cockpit verification: 583 tests / 69 files passed.
  Cockpit production build passed. Browser verification is running.
- R2: explicit mutation-root classification replaces the nested root expression.
  Workspace disposition and ownership commands no longer require an unrelated
  company. Company-knowledge writes now participate in the documented canonical
  actor gate. Baseline registry regression: 7 failures / 8 tests; one existing
  knowledge lifecycle still passed. Exact disposition confirmation and two-company
  authorization/lifecycle/retry tests added. Reopen's explicit/derived identity
  distinction is characterized and preserved, rather than silently hardened.
  New scope invariants use public command specs to detect future omissions.
- Disposition inspection reuses the existing strict snapshot reader, including
  committed WAL frames without touching original sidecars. The fixture's initial
  raw-main-file hash assertion was invalid before checkpointing its own writes:
  drift already occurred on an authorization denial that never inspected legal
  ledgers. The corrected fixture finishes writes and hashes database plus all
  sidecars; successful and denied lifecycle calls preserve that digest. Do not
  describe the original hash mismatch as a proved domain mutation.
- R3: supplier mapping, recipient context and uniform 25% arithmetic now have one
  owner, with caller validation positions and rules unchanged. A before-change
  matrix of ten overlapping-invalid cases established distinct first errors and
  no accounting/audit/sequence writes. All 44 focused purchase tests / 6 files
  passed after extraction; independent source review found no supported drift.
- R4: six invoice mutation modules use one structured transaction-rejection error
  and legacy decoder. Transaction ownership, rollback, rule union and credit-note
  file cleanup are unchanged. The JSON message remains compatible. Malformed
  legacy details outside the typed result contract now fall back to the original
  error rather than producing character rules or string errors. Focused invoice
  suites: 30 tests / 6 files; additional codec, fault rollback, concurrent caps,
  credit-note and MCP lifecycle suites: 39 tests / 5 files; all passed. Independent
  source/legacy-decoder review found no supported drift.
- R7/R8: unreachable Dinero branch and its private helpers removed; agent imports
  the canonical core formatter directly. Both active import preflights retained.
  Focused import/agent/money verification: 132 tests / 13 files passed.
- R9: one internal read-ledger function owns snapshot opening, schema gate and
  guaranteed close; public wrappers retain path/error/actor policy differences.
  Focused MCP verification: 55 tests / 3 files passed, including byte-preserving
  reads, concurrent read/write isolation, async failure close/redaction, schema
  refusal and slug resolution. Independent interface review found no substantive
  regression; close failures now also reach the envelope catch.
- Runtime typecheck and lint passed after core/MCP edits. Full backend verification
  is running. One focused multi-file CLI run failed at existing group mapping
  setup with empty stdout; stderr diagnostics were added and a focused rerun is
  running. Baseline bank-suggest-matches flakiness must still be revisited.

## Remaining completion work

1. Resolve or diagnose integrated failures; rerun changed checks when warranted.
2. Compare public discovery surfaces and document intended metadata corrections.
3. Run smoke, delivery/container and relevant end-to-end acceptance after changes.
4. Update Graphify and verify changed symbols, removed dead paths, semantic
   retention and graph integrity; refresh graph build metadata honestly.
5. Review the integrated result and audit each item and original goal requirement.
6. Record final evidence and remaining limitations before marking completion.

## Delivery-base reconciliation

Delivery fetch found origin/main at `2896aafe`, with substantial supported
functionality absent from the original `codex/release-render-fix` checkout
(479 non-graph files differ; 329 src/app files). Its local green checks therefore
prove preservation only against `42a91717`, not the current product. Do not
mark the goal complete or publish a main-target PR from that old base.

The completed source work and evidence above are retained. Product diff is
saved locally at `/tmp/rentemester-refactor-x57lVC/refactor-product.patch`,
including separate untracked helpers/tests to transfer. The next bounded step
is to port the same nine improvements into a managed worktree of current
origin/main, retain every newer safeguard/feature, rerun the baseline and
integrated acceptance there, update its graph and deliver verified changes.
No selected item is removed or weakened by this reconciliation.

Managed integration checkout created at
`/Users/mikkelfreltoftkrogsholm/.codex/worktrees/rentemester-simplification/rentemester`,
base `2896aafe`. Current-main baseline frontend passes 619 tests / 74 files;
backend baseline is running. Six tracked patches need deliberate adaptation;
others apply cleanly. New incomplete-invoice VAT review controls must be retained.

The old-base full `verify:local` passed through smoke/container integration but
its final container reproducibility run failed with two different digests. A
separate reproducibility run had passed. Cause is unproven; source/artifact changes
during the verification window are a possible confounder. No claim of a full
green old-base verify gate or current-main completion is supported.

This old-base checkout is explicitly parked as a local preservation checkpoint.
After transfer, the progress record in the managed current-main checkout is the
authoritative continuation record; this file is historical and must not govern
new completion claims. No remote push or PR is produced from this older base.
