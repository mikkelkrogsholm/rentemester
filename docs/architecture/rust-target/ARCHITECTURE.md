# Rentemester Rust target architecture

Status: original proposed target design, 2026-10-17; desktop presentation superseded by the user-approved 2026-10-20 [decision](DESKTOP-UI-DECISION.md). The model remains a planning artifact, not runtime evidence. The GDD model is gdd.json; graph.html is derived from that model. This document explains its decisions, not an implemented Rust product.

## Product outcome

One accounting product supports a browser connected to a server, a native desktop connected to that server, and a native desktop operating entirely on the user's computer. Local use needs no separately installed Bun, Docker or database server. External services still require connectivity when explicitly invoked; core local accounting does not.

The destination is Rust for the backend, CLI, MCP and native desktop host; the existing React/TypeScript UI is shared by the browser and embedded desktop presentation. The user-approved host direction replaces separate GPUI screens. Tauri 2 is the selected Rust host framework; embedding remains subject to platform, accessibility, security and distribution qualification. The proposal retains a modular application, one deployment per customer organization, and separate ledgers per legal entity.

## One execution path

Web React UI -> HTTP API -> application services -> accounting domain + storage/integration adapters.

Desktop Rust host -> embedded existing React UI -> either managed local HTTP runtime or remote HTTPS API -> the same application services.

CLI and MCP -> the same application services locally, or the API client for an explicitly selected remote connection. Neither interface owns separate accounting rules.

The initial desktop intentionally uses the HTTP boundary even locally. A managed child process runs the same backend executable used for server deployments; the desktop owns its startup, readiness, authenticated loopback channel and shutdown. This costs a local process and serialization, but gives all graphical clients one contract and avoids a second embedded write path. In-process embedding can be reassessed only if measured needs justify it.

## Components and proposed code organization

These are module boundaries. They need not all become separate crates; create a crate only when independent compilation, reuse or dependency isolation benefits an actual consumer.

| GDD component | Responsibility | Exclusions |
| --- | --- | --- |
| component.domain | Money, dates, currencies, journal invariants and accounting policy evaluation | HTTP, UI, SQL, credentials, external network |
| component.application | Use cases, trusted actor/company context, permissions, policy gates and atomic mutation orchestration | UI presentation and transport-specific authentication |
| component.storage | SQLite transactions, migrations, audit persistence, immutable evidence files, snapshots and restore | Business authorization and client-side database access |
| component.integrations | External provider protocols, import formats and durable effect delivery | Independent ledger mutation or invented accounting decisions |
| component.runtime | HTTP API, authentication translation, version/capability discovery, server lifecycle and task execution | Duplicated accounting rules |
| component.agent | CLI/MCP contracts, discovery, structured results and explicit local/remote selection | Independent write policy or hidden company selection |
| component.desktop | Rust host, embedded shared React UI, connection profiles and managed local runtime | SQL, accounting authority and automatic offline replay |
| component.web | Shared React browser/embedded-desktop screens using the API | Direct ledger access and privileged desktop lifecycle |

An initial Cargo workspace can contain rentemester-engine (domain/application/adapters as modules), rentemester-server, rentemester-client, rentemester-cli, rentemester-mcp and rentemester-desktop. Exact crate layout and third-party libraries remain implementation decisions. Reusable provider adapters stay small and are extracted only when needed.

## Contracts

Transport requests carry explicit workspace/company IDs, correlation IDs, supported API version and typed payloads. Mutations also carry an idempotency key, required explicit confirmation, and an expected revision when changing a reviewed object. Trusted identity comes from the local host or authenticated server session, never a caller's claimed actor field. Local CLI actor names remain checked against policy; hosted actors are derived from authenticated principals.

Version negotiation is read-only and precedes workspace access. The server returns protocol version and capabilities. Incompatible clients refuse dependent actions with an actionable error. Optional features are negotiated; unavailable functions remain visibly unavailable. Wire money uses exact decimal/integer strings with currency and explicit scale rules; no JSON floating-point money authority. Bounded Rust numeric types must reject overflow rather than wrap. Current bigint behavior, rounding, serialization and hash canonicalization require an explicit compatibility matrix before selection of numeric representations.

All writes pass through application services: authorize company access, validate confirmation and expected revision, enforce closed-period/backup gates, evaluate domain rules, then persist accounting changes, idempotency result and audit atomically in the company ledger. Authorization checks must not become stale across membership changes; cross-database control/ledger concurrency semantics require a specific implementation contract before hosted writes are ported. Replays reauthorize current access before returning a stored result. The same key with different canonical input is a conflict. A lost response does not cause a new key or blind replay.

A posting transaction cannot atomically send email or call a bank. External delivery uses a committed effect record and provider-specific retry/deduplication rules. Ambiguous provider outcomes stay unresolved until reconciliation; do not promise universal exactly-once delivery. File ingestion stages and verifies immutable content before ledger publication; crash recovery must distinguish unreferenced staged files from committed evidence. Do not imply SQLite makes arbitrary filesystem writes atomic.

Errors distinguish invalid input, denied access, conflict, policy refusal, unsupported capability and transport uncertainty. CLI preserves meaningful exit semantics and machine-readable envelopes. MCP tools expose the same preconditions, effects and error information through their protocol.

## Data ownership and operating modes

A workspace contains organization control data and company directories. Each company retains its own SQLite ledger, evidence originals, policy/configuration, exports and backups. Concrete company identities, mappings, amounts and ownership graphs belong to the workspace. Consolidation is a read-only reporting layer with explicit eliminations.

In local desktop mode the data lives in an explicit user-selected directory. The managed runtime binds only loopback, uses an ephemeral private connection credential, and never accepts hosted credentials as a local fallback. A workspace has one owning runtime; a second desktop/CLI writer connects to it or receives a clear busy result. Lock recovery must verify ownership and liveness rather than delete a lock speculatively.

In remote mode data stays on the server. Desktop credentials are held by the OS credential store and scoped to a server/account. Local mode is never an automatic fallback after remote network failure. Connection/company identity stays visible; switching connection cancels pending reads and invalidates caches scoped to the old identity. Remote logout/revocation must invalidate access consistently in web, desktop and agents.

The shared web UI is served under the authorized API origin. Its embedded desktop session must retain browser authentication guarantees; the host owns connection/origin isolation and must not invent a separate password/token scheme or expose privileged host operations to remote content. TLS/proxy, session revocation, MFA, membership, last-owner protections, CSRF, rate limiting and audit are parity obligations.

## Migration and release boundary

Keep the current product operating while introducing bounded Rust slices. First establish behavior inventory and read-only report parity on synthetic workspaces. Then port posting and refusal paths, then remaining accounting flows and integrations, then server/auth and client coverage. A small installed-host read-only slice can qualify embedding, navigation isolation, session handling and file/PDF integration before enabling financial writes. Historical GPUI qualification is not evidence for the new host.

There is one authoritative writer per workspace during transition. Never dual-write TypeScript and Rust into a live ledger. Compare copies/synthetic fixtures and use a capability-routing boundary with explicit ownership for each enabled path. Any coexistence arrangement must preserve single runtime ownership; otherwise it is not allowed.

Prefer retaining SQLite and evidence layout, but do not claim byte compatibility until migrations, constraints/triggers, audit hashes, signatures, canonical JSON, paths and restore behavior pass verification. Initial Rust readers open supported schemas read-only without migration. Unsupported schemas refuse access.

Cutover requires a complete feature/permission/error coverage inventory across CLI, MCP, web and desktop, verified backups and restore, accounting and audit parity, deployment checks and the existing production gates. A snapshot is verified in a staged destination before publication. Secrets and membership are re-provisioned according to the destination profile. Rollback uses the last verified pre-cutover snapshot; reopening a database written by an incompatible new version is forbidden. Preserve newly committed activity for explicit reconciliation before rollback.

## Non-goals for this boundary

No offline editing of a server-owned ledger followed by synchronization, multi-master database, new SaaS control plane, PostgreSQL migration, rewrite of React into Rust, microservices, custom authentication cryptography or enterprise plugin framework. Local-to-server transfer is an explicit verified migration, not file synchronization.

## Open decisions

- Native and browser identity provider strategy: retain Better Auth temporarily behind an explicit auth boundary or replace it with a supported service/library. A temporary TypeScript auth service means the runtime is not yet entirely Rust; it must remain visible in completion criteria.
- Exact schema range, monetary bounds/scales and audit/signature compatibility matrix.
- Revocation serialization across workspace-control and company-ledger databases.
- Tauri 2 host and embedded UI platform/accessibility/security qualification, signed packages and update lifecycle. Target macOS, Windows and Linux; claim support only after platform checks.
- API version policy and support window for older desktop clients.

These questions block their dependent implementation slices, not delivery of this proposed architecture.
