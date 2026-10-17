# Rust implementation progress

Active branch: codex/rust-platform. Goal: complete epic #672 and all 55 reviewed delivery issues; this increment is not the final product.

## Current verified increment

- Cargo workspace with engine, storage and experimental rentemester-rust CLI; Rust 1.94.1 pinned, Cargo.lock tracked.
- Pure arbitrary-precision money/date/report logic. Legacy REAL decoding uses ECMAScript number spelling; output amounts are exact decimal strings.
- No-follow directory-relative Unix source capture, bounded stable DB/WAL/SHM/journal copies, private recovery and current v36 history plus direct-table metadata checks.
- Strictly read-only trial-balance command. It never opens originals through SQLite or migrates them. Unsupported schema, unsafe legacy amount/ID precision and invalid evidence shape refuse explicitly.
- Seven Rust unit tests and 34 synthetic integration checks pass, including full-schema legacy parity, independent totals, UTF-16 sorting, WAL preservation, concurrency and refusal counterexamples. 29 targeted legacy tests pass.
- Independent review found three material defects; all were fixed, covered by counterexamples and re-reviewed without remaining material findings in this scope.

Run `bun run rust:verify`. The development harness finds Cargo on PATH or in the standard rustup location. For the experimental CLI: `cargo run --locked -p rentemester-cli --bin rentemester-rust -- trial-balance --company /path/to/synthetic-company --from 2026-01-01 --to 2026-12-31`. Use supported synthetic/copy data while this migration is incomplete.

The complete scoped contract and current readiness report are in read-only/. Historical normative v1 assets remain in read-only/history/v1/. Evidence/report.json binds exact current code hashes and states its limits.

The next runtime increment adds `rentemester-rust verify-journal-hashes --company PATH`. It reuses the same source capture, reconstructs legacy canonical journal JSON and verifies predecessor/hash continuity without source writes. Eight Rust unit tests and 29 dedicated hash integration checks pass, including real TypeScript post/reverse vectors, Unicode/NUL/FX/large-number bytes, damaged-copy refusals and 12 concurrent real postings. The previous 34 read-only and 31 wire checks still pass; 20 targeted legacy journal/integrity tests pass. Contract, GDD readiness, fixed vectors and current evidence are in journal-hashes/. This command does not replace the full legacy audit verifier or prove audit_log/document/accounting integrity.

The runtime foundation now includes rentemester-runtime::WorkspaceLease: a cooperative exclusive directory flock with private CLOEXEC handle, typed busy/refusal, kernel crash release and sticky root-identity invalidation. Twelve Rust unit tests and ten process checks pass on macOS/arm64/APFS, including real contenders, crash/EOF, aliases, moved roots, PID/metadata irrelevance and independent Python protocol interoperability. The current executable consumer is a development example; no product writer or legacy ingress uses this lease yet. The workspace-lease/ decision and evidence retain mandatory full legacy admission, physical/logical scope binding, offline adoption, IPC and platform/transaction gates before writes.

## Baseline status

The initial registry capture in baseline/inventory.json contains 265 CLI keys, 214 MCP tools, 208 HTTP routes, 54 browser routes, 734 exported core function declarations and 66 migrations. It is reproducible with scripts/rust/capture-baseline.ts and has explicit UNKNOWN fields. R01 remains open: complete per-operation input/result/error/fixture and domain ownership mapping is still required. The original snapshot is preserved; subsequent product/source changes must be reconciled, not silently overwritten.

The first executable wire-contract overlay is baseline/TRIAL_BALANCE.md and baseline/trial-balance-wire.json. It covers legacy CLI streams/exits, HTTP live/archive/error shapes, CSV byte output, limited PDF format checks and the explicitly versioned Rust CLI. It reproduces padded-year and noncalendar fiscal-year defects as correction obligations. The frozen synthetic oracle is hash-checked after every operation has run on disposable copies. These checks now run in rust:verify; full R01/R08 acceptance remains outstanding.

## Next work and completion boundary

Finish the baseline operation/schema/error/retry matrix and its independent positive/negative fixtures, refine the next GDD scope, then continue the reviewed DAG toward durable storage/authority/posting and remaining domains. Preserve existing user changes. All broad epic issues remain open until their full acceptance is actually proven. Hosted identity, providers, GPUI/React parity, distribution, actual external gates and fenced cutover remain outstanding. No production data was changed and no future unattended job was scheduled.

The company migration rehearsal now adds `rentemester-rust rehearse-company-migration --company PATH`. It executes exact existing migrations33–36 only on private disposable captures for strict starting prefixes32–36, checks affected canonical schema, prior provenance and typed business-cell digests, then explicitly closes/deletes the copy before reporting success. Seventeen Rust tests pass (one subprocess helper is intentionally ignored in normal counting and invoked by its parent); 25 new integration checks compare all five synthetic suffix fixtures with independent TypeScript migration results, exercise refusals and verify source/WAL preservation and empty private directories. Actual process-death tests cover SQL/history/commit boundaries; v36 receivable constraints are exercised in Rust. Current readiness, contract, raw results and code-bound evidence are in migration-rehearsal/. These constructed fixtures do not qualify historical archives, older/baseline/control migrations, bank-correction behavioral parity, live writers or full R12 acceptance.
