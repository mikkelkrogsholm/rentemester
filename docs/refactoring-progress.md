# Rentemester: behavior-preserving simplification

## Mandate and delivery boundary

Make the complete product easier to understand, change, test and diagnose.
Preserve supported UI/CLI/HTTP/MCP behavior, existing data, accounting rules,
actor/access policy, confirmation, audit history, atomicity and idempotency.
No new features, schema/rule changes, company-specific defaults, merge or
production deployment. The lead owns implementation and verification.

Delivery branch: `codex/rentemester-simplification`, based on current-main
commit `2896aafe99cec716325bdfc3a9949e24dc2bd90a`. The source tree, graph health
and final verification are recorded in `graphify-out/refactor-verification.json`.
That machine-readable record supplies the final gate status and exact counts;
temporary logs are supporting local evidence, not product prerequisites.
That record is historical refactor acceptance. The subsequent requested review
and defect repairs are recorded in [review-adversarial-2026-10-03.md](review-adversarial-2026-10-03.md)
and `graphify-out/review-verification.json`.

## Finite improvement list

| ID | Delivered simplification | Observable acceptance |
| --- | --- | --- |
| R1 | One cockpit transport owns JSON/binary requests, authentication expiry and structured failures. | PDF/tar bytes, headers and envelopes retained; one request; binary auth/body/network failure and read/write distinctions verified. |
| R2 | Explicit CLI mutation-policy scope; workspace handlers own their precondition sequence. Canonical identity precedes registry writes and disposition access. Repeated period actor resolution removed. | Workspace targets, confirmation/error precedence, actor denial/attribution, four access bootstrap scopes, three knowledge writes, ownership lifecycle and period explicit/derived actor distinction verified. |
| R3 | One owner for purchase supplier/recipient evidence and native-currency 25% arithmetic. | Direct-bank and payable first-error order, incomplete-invoice review differences, mixed/simplified VAT, FX, tolerance, recipient evidence and no-write denial verified. |
| R4 | One structured transaction-rejection error and legacy decoder across invoice, interest correction, asset, payable and accrual mutation paths. | Owning transactions, rollback, audit/sequence continuity, outer receipt rethrows, existing rule merging and legacy messages retained. |
| R5 | Five cockpit views use `useAsync.reload` instead of refresh counters. | Same-resource pending/failed reads retain data; actual resource changes clear data. Legacy bank-binding modal refresh and integrity stale-result warning verified. |
| R6 | One exact invoice serializer for preview and issue. | Identical bodies and existing optional-field omission/zero-VAT semantics retained. |
| R7 | Unreachable legacy Dinero archive/bilag branch and private helpers removed. | Both active preflights and atomic v4 effects retained; dry-run, rejected provenance, file rollback, links/open items and immutable retry verified. |
| R8 | Agent loop calls the domain money formatter directly. | Danish output retained; runtime no longer imports the CLI rendering barrel. |
| R9 | One MCP strict read-ledger lifecycle owns opening, schema validation, awaited execution and close. | Public wrappers retain distinct path/error/actor policies; read-only byte preservation, async close/redaction and concurrent read/write isolation verified. |

This list remains the finite work boundary. R4 includes the identical remaining
codec sites found on newer main; this does not add a new improvement category.
Tests characterize observable contracts rather than implementation layout.

## Product-area assessment

Graphify queries and scoped source/diff review guide this assessment. The graph
is a navigation aid: structural edges can be collapsed or undirected, and
semantic inference is checked against sources before changing behavior. This
assessment covers product areas; it does not claim a line-by-line proof of every
unchanged function.

| Area | Assessment and disposition | Verification evidence |
| --- | --- | --- |
| Ledger, schema, money, VAT and audit | Preserve domain rules/schema and transaction ownership; selected shared seams R3/R4/R8. | Full backend; purchase rejection/acceptance matrix, fault rollback, sequences and money tests; CLI/MCP lifecycle smoke. |
| Invoices, claims, refunds, credit notes, interest, payables, assets and accruals | Share mechanical rejection encoding only; keep workflow rules and result-specific rule composition. | Lifecycle, corrections, negative/fault cases and receipt-idempotency rollback suites. |
| Imports, bank evidence and external integrations | R7 removes proved unreachable code. Retain both preflights, provenance, v4 and fan-out gates. | Archive/file rollback, evidence identity and workspace DigiSense governance suites. |
| CLI, workspace registry, groups and periods | R2 makes scope ownership explicit; preserve each existing handler's validation order and current company policies. | Public-spec scope checks, registry/disposition lifecycle, actor/confirmation matrix and period tests. |
| HTTP/MCP security, discovery and write orchestration | R9 consolidates reads. Keep newer service-principal membership, authenticated idempotency, confirmed-write and backup-lock wrappers. | Discovery comparison, server/security suites, MCP strict-read/concurrency tests and smoke. |
| Cockpit routes, forms, API and export | R1/R5/R6 share existing mechanisms, including both accountant-export placements and legacy bank callback. | Full cockpit/browser acceptance; binary transport, invoice modal, export replay guard and deferred refresh cases. |
| Reports, consolidation and readiness | Preserve read-only reporting layers, explicit eliminations and existing readiness controls. | Full backend, group/period/readiness suites and report smoke. |
| Runtime agent and operational tooling | R8 removes CLI dependency; retain runtime gates and packaging workflow. | Runtime/discovery, typecheck/lint, supply-chain, container integration and OCI reproducibility. |
| Public website and documentation | No website behavior change selected. Update Graphify workflow and evidence documentation. | Structural scan and diff review; packaging includes final docs. |

## Deliberate non-changes

- Keep direct-bank expenses and payables separate: FX, treatment, internal
  vouchers and validation sequence have different supported contracts.
- Keep ordinary and recurring invoice validation separate: quantity and price
  rules differ.
- Keep the outer Dinero archive preflight: removing it can change rejection
  envelopes and rejected-attempt provenance independently of dead-code removal.
- Keep company-local supplier policy/account mapping unchanged: changing that
  boundary would change supported product behavior beyond this mandate.
- Retain newer main's purchase-case, party/master-data, authenticated write,
  readiness and service-principal controls. Port helpers into current files;
  never replace them with older release-branch versions.

## Baseline, failures and repair evidence

Current-main baseline passed frozen installation, runtime typecheck, lint,
production cockpit build, 2,888 backend tests / 454 files and 619 cockpit tests
/ 74 files. Discovery baseline: MCP 249, CLI 299, HTTP 241, 29 capabilities,
42 workflows and 789 bindings.

The original release checkout (`42a91717`) was 479 non-graph files behind
current main. It is parked as local historical commit `0bbe82bc`; it supplies
no current-main delivery proof. Its full suite had a bank CLI flake, and its
full packaging gate failed reproducibility despite a separate successful run.
No old-base green claim or main-target PR is based on that result.

Before-change characterization established purchase error ordering and denial
without accounting/audit/sequence writes. Binary transport, export replay risk
and refresh data loss have failing-before/passing-after evidence. Invalid
snapshot fixtures were corrected before using them as proof: a nonexistent
context `id` key and hashing unfinished fixture WAL writes were fixture errors,
not demonstrated product mutations.

The first current-main full gate passed (2,922 backend, 634 cockpit, 177 browser
tests). Final independent review subsequently found two uncovered defects:
central handler actor validation changed confirmation/target error precedence,
and IntegrityView hid retained data during refresh/failure. Those findings
invalidate use of that earlier gate as final proof. The handler sequence is now
restored, identity remains before registry mutation/disposition reads, and the
integrity view retains the last result with a clear refresh/failure indication.
Focused repaired policy acceptance: 61 tests / 6 files; integrity/hook: 14 tests
/ 2 files. The new CLI matrix failed ten cases before the repair.

The newer interest-correction codec was also identified during final review.
Its 37 existing tests passed before extraction and the 40 interest/codec tests
passed afterward. Interest correction retains its fixed bookkeeping rule in
failure results. Asset/payable/accrual codecs retain their transaction mode,
rule merging and payable outer-transaction rethrow behavior.

Discovery counts and catalogue stay unchanged. Exactly three company-knowledge
CLI bindings are corrected from read/no-actor metadata to write/actor/unsafe
read-back; derived coverage/profile hashes change for this intended correction.

## Graph and final verification evidence

Graphify is installed at 0.9.74 with the SQL parser. The graph includes the
current structural extraction and semantic document/rule/image evidence.
Ninety-eight unchanged semantic sources reuse content/prompt-matching cache
fragments; 25 changed/new sources were freshly extracted. Luna's first fragments
were insufficient (truncated reads and a chunk without edges); those results
were rejected. One Sol repair supplied 140 unique nodes and 97 located edges,
with all 25 sources represented and valid endpoints. Final edits to this record
and the Graphify guide are source-checked before cache/manifest stamping.

Native semantic token usage is unavailable. Zero token placeholders are not a
claim of zero cost. `graphify-out/build-info.json` records source fingerprints,
coverage, actual provenance and limitations; `graphify-out/graph-health.json`
records graph diagnostics, including structural parser/direction limitations.

Completion requires the final `bun run verify:local` to exit 0 on a frozen
source/docs tree, graph integrity and semantic coverage to be checked, and each
R1–R9 item to have source and acceptance evidence. Consult
`graphify-out/refactor-verification.json` for that final audited status.
