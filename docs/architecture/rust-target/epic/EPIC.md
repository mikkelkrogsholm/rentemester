# EPIC: Rust-platform for Rentemester — server, web og lokal/remote Rust-desktop med fælles web-UI

<!-- rentemester-rust-plan:RM-RUST -->

## Product outcome

One shared Rust accounting platform supports a server with React web UI, a Rust desktop host embedding the existing React UI and connected to that server, and the same desktop with an automatically managed local Rust backend. The final backend, CLI/MCP, identity path and packaged document workers require no Bun/Node runtime. React build tooling can remain.

The installed desktop uses the same API in local and remote mode. Each legal entity keeps its own ledger and evidence; one runtime owns a writable workspace. Data transfer is an explicit verified migration, not offline synchronization.

## Approved desktop presentation update — 2026-10-20

The user explicitly approved replacing separate GPUI screens with a Rust host embedding the existing React UI. See [the decision](../DESKTOP-UI-DECISION.md). Tauri 2 is the selected host framework; installed local/remote qualification remains open. Historical GPUI work is retained as evidence of its original scope, not completion of the new host. All child IDs, accounting/security gates and J01–J07 scope remain intact.

## Agreed direction and limits

The original user request was a comprehensive epic, local planning first, review/fix loops, then GitHub issues. Implementation was subsequently authorized by the active Rust goal; live cutover remains separately gated. Rust/shared React presentation/server/local/remote are the approved target direction; exact identity/toolkit/numeric choices are resolved by the early decision issues.

## Completion criteria

- [ ] One shared Rust accounting engine with a versioned API for web, local desktop and remote desktop.

- [ ] Retain React; use the qualified Rust host with the existing React UI for desktop presentation; core local accounting needs no separately installed Bun/Node/Docker.

- [ ] One runtime owns each writable workspace; legal entities keep separate ledgers and evidence.

- [ ] No company-specific code/defaults; synthetic checked-in test data only.

- [ ] Existing correctness/security/backup/release obligations remain binding; known defects require explicit corrected contracts.

- [ ] No remote-offline edit/sync, multi-master, microservices, PostgreSQL move or new SaaS control plane.

- [ ] No implementation issue closes based only on GDD validity, a test label or a worker claim.

- [ ] Named owner journeys J01–J07 pass; native core tasks cannot be waived into generic specialist exceptions.

- [ ] All child issues below are complete with applicable revision-bound evidence; unresolved product/security/provider gates block affected release claims.

- [ ] Baseline drift, known defects and unfinished unrelated enhancements have been reconciled; no silent feature loss.

- [ ] Synthetic full-year accounting, cross-interface parity, installation/update and fenced cutover/rollback rehearsals pass.

## Architecture and design decisions

Modules: pure domain; application use cases and authority; storage; provider integrations; HTTP runtime; CLI/MCP; Rust desktop host; shared React presentation. Start with the smallest useful Cargo boundaries. No microservices or speculative framework.

Early binding decisions: schema/money/audit compatibility (R03), identity and revocation ordering (R04), versioned wire protocol and old-client support (R05), native accessibility/platform feasibility (R06). These are named deliverables, not permission to guess.

## Execution rules

Use R01 to freeze and continuously reconcile the actual baseline. R02 refines each slice into a complete GDD contract before coding. Planned acceptance is not executed evidence. Known bugs are corrected explicitly rather than preserved blindly. No feature/domain write is enabled until its full authorization, evidence and validation dependencies are ready.

Keep the current product working. Develop against synthetic/copy fixtures. No concurrent legacy/Rust writes to a live workspace and no partially enabled hybrid writer. Enable production Rust only after the complete acceptance/cutover gates; a read-only installed-host embedding slice can run early.

## Child issues and sequencing

### P0 — Inventory and binding design decisions

- [ ] #673 — R01: Freeze the migration baseline and complete the feature/contract inventory

- [ ] #674 — R02: Specify GDD contracts, component boundaries and per-slice readiness

- [ ] #675 — R03: Define exact money, schema, audit and wire compatibility

- [ ] #676 — R04: Resolve hosted/native identity and cross-database authorization ordering

- [ ] #677 — R05: Specify versioned API, typed failures and capability negotiation

- [ ] #678 — R06: Qualify Rust desktop host, embedded UI and target-platform feasibility

### P1 — Rust foundation and first read-only vertical slice

- [ ] #679 — R07: Bootstrap the Rust workspace, build tooling and architecture checks

- [ ] #680 — R08: Build the synthetic differential and fault-injection acceptance harness

- [ ] #681 — R09: Implement exact accounting value types and invariant evaluation

- [ ] #682 — R10: Implement a strictly read-only SQLite/workspace adapter

- [ ] #683 — R11: Deliver the first Rust vertical slice: read-only trial-balance parity

### P2 — Durability, authority and atomic writes

- [ ] #684 — R12: Implement workspace ownership, transactional storage and safe migrations

- [ ] #685 — R13: Port workspace/company authorization and revocation fencing

- [ ] #687 — R14: Port atomic posting, reversal, idempotency and shared mutation gates

- [ ] #686 — R15: Port immutable document storage, safe ingestion and evidence access

### P3 — Accounting and business capability parity

- [ ] #688 — R16: Port accounts, contacts, parties and supplier identity

- [ ] #689 — R17: Port VAT rules, purchase evidence, filings and reconciliations

- [ ] #692 — R18: Port invoice issuance, immutable PDFs and invoice numbering

- [ ] #694 — R19: Port receivables, settlement, credits, FX and overdue claims

- [ ] #695 — R20: Port purchases, payables and supplier commitments

- [ ] #693 — R21: Port canonical bank import, matching and reconciliation corrections

- [ ] #696 — R22: Port posting rules, exceptions and bank-based bookkeeping workbench

- [ ] #697 — R23: Port versioned drafts, batch review and exact approval/apply

- [ ] #698 — R24: Port fixed assets and depreciation

- [ ] #699 — R25: Port accruals and mileage accounting

- [ ] #700 — R26: Port period readiness, closing and opening balances

- [ ] #701 — R27: Port historical imports, provenance and migration open items

- [ ] #702 — R28: Port dimensions, budgets, commitments and CFO analytics

- [ ] #704 — R29: Port ownership graphs, intercompany and consolidation reporting

- [ ] #705 — R30: Port company knowledge, corporate records and workspace inbox

- [ ] #706 — R31: Port annual accounts, tax and statutory export formats

- [ ] #703 — R32: Port financial reports, dashboard and read projections

- [ ] #707 — R33: Port backup, restore, signatures and workspace transfer

- [ ] #708 — R34: Port retention, GDPR and versioned regulatory evidence

### P4 — Providers, identity and runtime

- [ ] #690 — R35: Implement durable jobs, external effects and restart recovery

- [ ] #709 — R36: Port explicit email delivery and recurring invoice automation

- [ ] #710 — R37: Port bilagsmail, EML and IMAP intake

- [ ] #691 — R38: Replace PDF parsing/rendering, extraction and scanning runtime dependencies

- [ ] #711 — R39: Port DigiSense, NemHandel and external reference adapters

- [ ] #713 — R40: Integrate the existing BYO bank roadmap through Rust adapter contracts

- [ ] #712 — R41: Implement hosted identity, native login and service-principal lifecycle

- [ ] #714 — R42: Implement the Rust HTTP server and operational API pipeline

- [ ] #715 — R43: Implement the Rust API client and connection-scoped caches

### P5 — Agent, web and desktop interfaces

- [ ] #716 — R44: Port the CLI, local launcher and machine-readable contracts

- [ ] #717 — R45: Port MCP, capability discovery and agent workflow parity

- [ ] #718 — R46: Retain React cockpit parity against the Rust server

- [ ] #719 — R47: Build the Rust desktop host and managed backend lifecycle

- [ ] #720 — R48: Deliver shared desktop read workflows, reports and evidence inspection

- [ ] #721 — R49: Deliver shared desktop bookkeeping, review and administration workflows

- [ ] #722 — R50: Complete remote desktop login, profiles and reconnection

### P6 — Verification, distribution, cutover and retirement

- [ ] #723 — R51: Verify complete cross-interface, concurrency and security parity

- [ ] #724 — R52: Package signed desktop apps and safe application updates

- [ ] #725 — R53: Package the Rust server and preserve candidate/release provenance

- [ ] #726 — R54: Rehearse fenced cutover, local/server transfer and rollback

- [ ] #727 — R55: Retire the TypeScript backend and complete documentation/support handover

## Parallelism and critical path

After inventory/contracts, Rust desktop-host qualification and pure Rust foundation/parity can proceed independently. After atomic storage/authority, domain ports can proceed against fixed interfaces. Providers, agents and clients integrate only after their actual contracts exist. Dependencies in child issues are concrete prerequisites, not permission to use stale implementations. Cross-domain files and shared schema/contracts require serialized ownership or isolated integration.

First demonstrable accounting milestone: R01 → R03/R07/R08/R09/R10 → R11 (read-only trial balance). Durable writes: R12/R13/R15 → R14. Final convergence: R51 verification + R52 desktop packaging + R53 server packaging → R54 rehearsed cutover → R55 retirement. R02 readiness applies before each implementation slice.

## Existing backlog

Existing Open Banking epic #363 remains authoritative for new provider functionality. Existing fixes such as #642/#643 and restore contract #664 must be reconciled in their relevant domains. Pending product enhancements are not automatically added to this rewrite. R01 and the per-issue references retain their real status.

## Local plan and evidence provenance

Base code commit: `0bbe82bcf0cafd73e2326152740cb06b90222405`. Base target-model revision: `sha256:a0c699bcb80ddd55c1ac9de4e8dd51e9972460f46ca682d37bbc91c9c4ea9d5e`. Planning package: `docs/architecture/rust-target/epic/` (local artifacts; not assumed published with the baseline commit).

The local package contains the machine-readable issue DAG, complete Markdown bodies, behavior/source coverage map, GDD delivery graph/view, validator outputs and review findings/fixes. GitHub bodies are self-contained; no unpublished local document is required to understand an issue.

No deadlines, effort estimates, unsupported platform claims, external approvals or implementation PASS results are invented.

## Named owner acceptance journeys

- [ ] J01 — Fresh local owner, VAT company. Given: Fresh qualified machine, no Bun/Node/Docker, synthetic VAT-registered company. When: Install, choose data directory, configure identity/fiscal/VAT/account plan/bank, ingest receipt, review/post, inspect report and verify/export backup. Expected: Owner completes normal setup/accounting without JSON/CLI, exact report/evidence links and off-device recovery instructions are visible. Owners: #719, #721, #723.

- [ ] J02 — Existing business, no VAT and shifted year. Given: Supported synthetic imported business, non-VAT configuration, shifted fiscal year and opening balances. When: Import/restore with explicit mapping, review open items, post first expense and inspect period/report/backup. Expected: No invented VAT or duplicate opening balances; unresolved historical evidence blocks correctly and the owner gets the next action. Owners: #701, #719, #721, #723.

- [ ] J03 — Remote multi-user operation and access loss. Given: Hosted workspace, two companies, owner/bookkeeper/service identities. When: Log in web/native, switch company, review/post, revoke access during a pending read/write. Expected: Same authoritative ledger, visible server/company and role-correct actions; no cross-company/stale-cache disclosure or local fallback. Owners: #712, #718, #722, #723.

- [ ] J04 — VAT/annual task state and required user action. Given: A synthetic period with VAT source evidence and an unresolved blocker. When: Review basis, resolve blocker, prepare/lock/record filing as actually supported. Expected: UI distinguishes draft/reviewed/recorded from submission, shows sources/deadline rules and the next manual action without false filing claims. Owners: #689, #706, #718, #720, #721, #723.

- [ ] J05 — Lost computer and verified owner recovery. Given: Separately held verified backup and trust material, then source device unavailable. When: On a fresh device restore to a fresh directory; re-provision secrets and inspect bank/evidence/journal. Expected: Exact supported inventory/audit restores, missing destination/key produces actionable help, owner/operator responsibilities are explicit. Owners: #707, #720, #721, #723, #726, #727.

- [ ] J06 — Local-to-server and server-to-local handoff. Given: A supported synthetic workspace with an accepted write whose response was lost, plus pending/ambiguous external effects. When: Stop/fence source, verify transfer, map identity/receipts, activate destination, retry original operation and reopen old source. Expected: One writer/dispatcher, no duplicate booking/delivery, replay obeys access policy; owner sees destination and moved/read-only source. Owners: #707, #722, #723, #726, #727.

- [ ] J07 — Local closure/sleep versus hosted automation. Given: Explicitly enabled supported recurring/intake/backup tasks in local and hosted profiles. When: Close app/sleep/change timezone or DST, then restart; revoke/cancel a queued task. Expected: Documented catch-up/skip and dispatch-authority policy, last/next/missed/pause/error visible, no implied unattended local service. Owners: #690, #709, #719, #723, #724.

## Dependency interpretation

Edges are completion prerequisites. Contract-driven scaffolding/prototypes may begin earlier with explicit stable interfaces; no dependent acceptance is claimed before prerequisites pass. Readiness is checked per bounded implementation package, not for an entire phase at once.
