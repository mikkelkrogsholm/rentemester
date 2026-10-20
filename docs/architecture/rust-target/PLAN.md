# Bounded development plan

This original execution sequence records the planning boundary. Implementation was subsequently authorized; the full active Rust goal is retained. The user-approved 2026-10-20 [desktop decision](DESKTOP-UI-DECISION.md) now governs presentation. This document schedules no future work.

## Slice 1 — read-only Rust accounting parity

Scope: action.report and its domain/storage boundaries, plus an explicit offline fixture runner. Read supported SQLite copies and compute a trial balance with exact money; never migrate or write the source workspace. Include two synthetic companies to establish isolation. Do not connect the new engine to real workspaces in this slice.

Proposed write ownership: crates/rentemester-engine/, crates/rentemester-cli/ (read-only fixture runner only), tests/rust-parity/ and dedicated Cargo workspace/build files. These paths are prospective; reconcile existing files and instructions before creating them. Keep existing TypeScript behavior fixed except for separately diagnosed defects. The current architecture bundle can be refined with a recorded new model revision.

Prerequisites: inventory supported schemas, currency scales, rounding, numeric bounds, report period/closing semantics and audit canonicalization; refine the contract and register normative sources/check plans. Resolve question.compatibility for this slice. No application writes, authentication replacement or desktop-host integration are needed.

Deliverables: a Rust read-only adapter, exact report implementation and an independently inspectable comparison harness. The TypeScript implementation is a comparison oracle, not sole authority; hand-calculated synthetic assertions establish the intended arithmetic.

Planned acceptance: exact totals for zero/negative/fractional and boundary amounts; unauthorized cross-company access denied; unsupported schema refused without migration; before/after byte hashes unchanged; deterministic report ordering; differences against both existing reports and fixed expectations explained. Existing defects are recorded explicitly rather than copied. Planned checks have not been executed.

## Subsequent slices

1. Transactional posting with balanced entries, audit, idempotency, closed periods, policy/confirm/backup gates, crash rollback and concurrent writes. Decide cross-database authorization/revocation semantics first.
2. Domain coverage: invoices, VAT, bank, assets, drafts/review, periods, import/export, evidence, backup/restore and group reporting. Build a complete existing-feature matrix before claiming replacement.
3. Runtime/identity and external effects: hosted auth parity, durable work/effect recovery, provider reconciliation and versioned API. Preserve all existing production gates.
4. Clients: qualify the installed Rust host embedding the existing React UI; complete local managed runtime and remote authentication, then enable writes only after shared contracts pass. Retain React and port its API integration where needed; preserve CLI/MCP discovery, permissions and error behavior.
5. Distribution and cutover: signed platform packages, server image, restart/update/migration checks, staged workspace transfer and recoverable cutover with one writer.

## Design review of the proposal

KISS: one backend and HTTP path for both graphical clients; modules before unnecessary crates. The local process is justified by contract reuse and workspace ownership.

DRY: accounting policy is authoritative in the shared engine. Reuse the existing React UI in browser and Rust desktop host. Keep host lifecycle/platform integration separate from presentation and accounting authority; protect behavior with API contracts and installed local/remote scenarios.

Separation of concerns: authentication translation belongs to runtime; business authorization and orchestration belong to application services; storage owns persistence mechanics. Unknown revocation races remain explicit.

Modularity: public actions expose provider-owned contracts. Desktop never imports storage; domain never imports UI/network/SQL. These are planned boundaries, not verified Rust import restrictions.

Original planning-stage implementation evidence was UNKNOWN. Later server/web/CLI/MCP and historical GPUI evidence is recorded in implementation/progress.json; it does not qualify the new shared-UI host. Model and viewer validation assess the planning artifacts only. The advisory review is prose and is not a host-attested design-review envelope.
