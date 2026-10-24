# Assets and accruals — existing #677

## Delivery boundary

58 frozen selected identities: 25 HTTP, 15 CLI and 18 MCP operations. Each row is `specified`, with `unresolved: []`, all eight contract dimensions and current source-byte SHA-256 bindings. Schemas use Draft 2020-12 `$defs`; references stay within this partition or accepted chart/journal definitions. No production files, dependency changes, runtime harness, inventory or other partitions are included.

The parent owns semantic review, integration and acceptance. This partition does not close #677. Prior 626 accepted operations in 22 partitions and all 55/J01–J07/final desktop/release/cutover/retirement scope remain retained.

## Important source distinctions

- Native assets/accruals HEAD requests hit the existing early 405 `Allow: GET` fence before credentials, body parsing or Owner access. Original report and disposal-receipt HEAD requests retain their original authenticated read dispatch; Axum strips response bodies. Global protocol negotiation still precedes handlers.
- Native direct HTTP Serde inputs admit full positional sequences in declaration order. Original HTTP uses an outer strict object parser. Every public MCP tool first passes `tools::call`'s object guard, so native leaf Serde sequence support does **not** make MCP arrays public input.
- Native SDK schemas model object-only validation separately from direct HTTP. Its validator counts UTF-8 bytes and does not evaluate advertised date regexes. The server still validates accounting dates and amounts. Original SDK checks exact numeric cents/calendar dates and imposes safe-ID projections; raw producers retain their signed-i64 IDs.
- Native registration responses differ from original cores: native accrual registration additionally includes its full schedule. Native HTTP envelopes, native CLI `data` wrappers, original CLI cores, original MCP lifted cores and disposal MCP raw cores are not standardized into one shape.
- Native generic SDK transport discards non-2xx bodies, including 422. Original asset/accrual transport recognizes validated business-false cores at both 200 and 422; asset preserves curated errors, accrual applies safe message projections. A malformed successful write response is uncertain, not proof of rollback. Disposal uses generic status handling plus a dedicated canonical receipt validator.
- CLI public argv passes original preflight and the global local binding before normalized dispatch. Split/attached global `--local-profile` binds the private matching Unix socket and exact logical origin, with no TCP fallback. Original commands accept attached value flags; generic remote commands do not. Recognized remote flags can be rejected by earlier service/source/report/budget guards. Non-TTY original output defaults to JSON unless human format is explicitly requested.
- Explicit depreciation/recognition periods refuse duplicate postings. Omitted/null native depreciation `periodIndex` selects the next period, so resending can create another posting. Registration has no general replay promise.
- Capitalized disposal is not immediate write-off. It preserves current plan/evidence/hash authority, explicit confirmation and caller key, principal/ledger/operation payload scope, 30-day outcome retention and durable tombstones. Receipt readback verifies retained disposal/journal evidence under current authority. Restore reconciliation is not unrestricted replay.
- Original immediate write-off may commit missing-document/threshold exception metadata while returning business-false. Service usage can commit separately from company accounting. Preview, refusal and read descriptions do not assert universal absence of metadata effects. Missing replies, changed authority and missing receipts do not establish rollback.

## Verification performed

1. Ran Graphify query before reading actual Rust sources.
2. Read selected HTTP, application, storage, SDK, MCP and CLI producers and global preprocessors; no server, Cargo, network, auth, provider or financial effects were executed.
3. Ran `python3 /tmp/r55-677-oracle.py assets_accruals`: **PASS**, 58 exact selected source-bound schema contracts. The oracle explicitly leaves semantic review to the parent and whole #677 open.
4. Ran `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`: succeeded, 34,009 nodes and 107,681 edges, generated observations confined to the independent build output.

One artifact-generation Python syntax error was corrected; no new test setup was introduced. Existing accepted runtime/user-flow proofs were not replayed. No live-provider approval is claimed.
