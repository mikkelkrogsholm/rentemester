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

Lead source review retained existing exact original appliedRules/no-code transport/domain refusal constructors (already correct in finalworker). HEAD success/refusal/protocol schemas are explicitly pre-strip constructors with zero wire bytes. Disposal receipt read has no clockexpiry check while outcome retained; apply replay30days remains distinct. DisposalMCP SDK validates inner confirm as invalid_input, never shared CONFIRM_REQUIRED.

## 2026-10-25 — Owner immediate write-off through shared React/Rust

The existing Straksafskriv form now binds explicitly to the accepted original
Rust writer. It selects a current active expense account and existing purchase
document, freezes a review before explicit confirmation, and validates the flat
journal/write-off receipt. The current configured bank role remains visible;
no client account default, VAT deduction or legal eligibility is inferred.
The source-bound AssetList.writeOffAvailable capability is now true. All1609
existing operations remain; no new engine or HTTP route was introduced.

A parsed exact source422 refusal may persist a supported exception. Malformed
receipts, transport failures and unknown writes retain the existing persisted
ConfirmDialog barrier. Reads never release it. Explicit human reconciliation
uses the existing release acknowledgement and sends no write. Main review
repaired a double-lock trap, native default/posted-close behavior and the
review context/receipt binding before accepting the candidate. Bun behavior
and shared transport policies for other callers remain.

56 distinct targeted cases passed:35 UI/API,20 shared confirmation/transport
and1 storage. The frozen Bun build invocation printed help; it is not build
evidence. A separate proper app-directory build ran design checks, TypeScript
and the existing bundler. An initial actual-browser selector failed before any
financial POST. The wrapped-label mismatch was reproduced independently; the
qualifier now uses the actual combobox accessible name. The failed profile and
logs remain, with zero journals verified before a fresh isolated fixture.

Actual React against Rust then verified normal two-user MFA, review and
explicit confirmation, one source refusal retaining an eligibility exception,
and a genuinely accepted response deliberately lost before the browser saw it.
The latter remained blocked across read, reload and reopening without resending.
Exactly2journals/4lines,2bill-linked write-off histories and3exceptions were
verified, followed by restart and signed source-free recovery. Original bytes
and exact history survived. The qualifier executes no CLI/MCP business matrix;
those older artifacts are supplied only for the reused lifecycle contract.
Synthetic explicit gross amounts are not tax advice or live-use acceptance.

This completes this bounded owner flow, not whole718. Other original screens,
capabilities, file/export/error/keyboard/local-hosted scenarios and J03/J04 must
retain full evidence. All55 original issues, release/cutover and final manual
installed-desktop acceptance remain. No live data or installed launcher changed.
