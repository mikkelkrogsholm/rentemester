# Archive and privacy ownership/recovery contracts (#677)

Source-only specification of the selected 48 operations. Main owns semantic review, integration and acceptance; whole #677, all 55 flows and J01–J07, final desktop/release/cutover/retirement remain open at their retained scopes. No runtime/userflow proofs were replayed.

Archive installation is immutable historical snapshot import, not live-ledger adoption. Plan reads actual originals and current history; apply binds exact original source bytes, review hash, current ledger identity and caller key. Published files and SQL commits are separate stages. Unknown outcomes are reconciled through canonical archives and original hashes, never treated as rollback.

Privacy export/discover are audited mutations and may strengthen pseudonymous erasure aliases. Legal hold lifecycle, signed audit export and retention report are separate scopes. Erasure is a scoped overlay: originals, journals, retained backups, provider data and other product views are not universally deleted. Coverage remains incomplete. Embedded audit signing metadata is not an independent trust anchor.

HEAD: these archive routes retain the original handler behavior, not the R54 early fences. Archive HEAD runs origin/cookie/selection/header guards before returning 405 invalid_input (no explicit Allow). Retention HEAD is treated as a mutation and attempts a confirmed body; its actual behavior is not a harmless GET alias. Axum strips HEAD response bodies.

CLI bare privacy and gdpr discover/forget/audit-log aliases normalize to remote/native stdin. No old TypeScript human-output/subject-selector/file/signing flags are admitted by these Rust aliases. Global local-profile is consumed before remote parsing; private Unix/exact-origin binding has no TCP fallback. MCP rejects argument arrays before dispatch, despite direct Serde sequence grammars.

## Verification

- `python3 /tmp/r57-677-oracle.py archive_privacy`: PASS, exact selected 48 operations, resolved source-bound schema checks; semantic review remains required.
- Required `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`: completed, 3,824 code files; generated graph outside owned code.
- Only four owned flat files changed. No Cargo, server, browser, network or effectful operation was run.

Schema scope: native producer shapes are specified rather than promoting permissive SDK `Value` readers into arbitrary top-level data. Failure message/code strings remain dynamic for transitive SQL/admission diagnostics; concrete HTTP mapping branches accompany those typed envelopes. Original source values are canonical six-decimal strings and money is separately quantized two-decimal text. Source refs are current SHA-256 byte bindings.
