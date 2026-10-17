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

## Baseline status

The initial registry capture in baseline/inventory.json contains 265 CLI keys, 214 MCP tools, 208 HTTP routes, 54 browser routes, 734 exported core function declarations and 66 migrations. It is reproducible with scripts/rust/capture-baseline.ts and has explicit UNKNOWN fields. R01 remains open: complete per-operation input/result/error/fixture and domain ownership mapping is still required. The original snapshot is preserved; subsequent product/source changes must be reconciled, not silently overwritten.

## Next work and completion boundary

Finish the baseline operation/schema/error/retry matrix and its independent positive/negative fixtures, refine the next GDD scope, then continue the reviewed DAG toward durable storage/authority/posting and remaining domains. Preserve existing user changes. All broad epic issues remain open until their full acceptance is actually proven. Hosted identity, providers, GPUI/React parity, distribution, actual external gates and fenced cutover remain outstanding. No production data was changed and no future unattended job was scheduled.
