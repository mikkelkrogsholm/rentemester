# VAT / annual / regulatory contract work — blocked, not accepted

This partition is incomplete. Do not integrate it as specified or count it toward #677 closure.

Graphify query ran first (`graphify query "vat annual regulatory"`). Actual Rust source was read across HTTP, application, storage, CLI, client and MCP adapters. No production changes, runtime probes, provider calls or financial effects were performed.

## Concrete delivery blocker

Two inline Python artifact-generation attempts failed before publication with `NameError: name 'REDACTED' is not defined` at the binary-download maximum-byte expression. The command received a redacted literal in that expression. Neither attempt wrote its in-memory definitions. Per the work-package two-failure limit, setup expansion stopped. The operation file is intentionally empty rather than fabricating specified contracts; the schema and provenance files explicitly record incomplete delivery. The lead must finish all selected operations and run the existing strict oracle successfully before acceptance.

## Source findings retained for repair

- Selected GET-backed HEAD routes in annual_exports.rs, annual_report.rs, compliance.rs, eu_sales.rs, oss_report.rs, vat.rs and vat_original.rs have no early HEAD fence. Their actual get() routing executes the read and Axum strips response bytes. Do not project R54 fences from unrelated handlers.
- Global MCP tools::call rejects non-object arguments before dispatch. vat_original::accepts runs before native dispatch: vat_report, vat_eu_sales_list and vat_oss_report require original company/from/to, never native year overloads.
- Original VAT GET parsing runs inside current authority; from/to may be omitted or empty (default empty strings) and malformed dates are business inputs. Strict query: at most 1024 bytes; each decoded value at most 128 bytes, no controls/replacement character, duplicates/unknown keys rejected. Content/transfer encoding and idempotency-key forbidden; content-length if present must be 0; body empty.
- Original VAT source ok:false is HTTP422 with core result retained by its dedicated SDK. Minimal nonregistration is {ok:false,errors:["selskabet er ikke momsregistreret"]}. Native generic GET SDK discards non-success bodies, so native422 is not equivalent to original422.
- Native annual HTTP200 outer ok:true can contain annualReport.report.ok:false. Tax/iXBRL annualExport can likewise be business false. No statutory filing is asserted.
- VAT purchase is CompanyLedgerPost, not report preparation. Strict payload and confirm:true envelope; optional actor is a current-principal assertion, not authority. Invoice selector resolves newest matching document and overrides supplied document ID. Representation omission of expense account requires unique configured active REPRESENTATION_SPECIAL expense mapping, never a fixed account number.
- Purchase actual HTTP receipt IDs are signed SQLite i64. Original client receipt projection requires positive safe integers. Purchase client preserves validated422 business core; 408/5xx, malformed200 or lost response become outcome_unknown. No idempotency key/replay guarantee; reconcile canonical journal/document/bank before another action.
- Native SAF-T and handoff HTTP Deserialize structs accept full declaration-order sequences as well as objects; MCP outer args remain objects. SAF-T sequence is periodStart,periodEnd,confirm,generationMode,generatedAt; handoff sequence is periodStart,periodEnd,confirm,packageProfile,requestedAt,requester. Inspect exact sequence optional-tail behavior before publication.
- SAF-T omitted generation mode uses trusted runtime clock; original system export-saft explicitly selects period_end unless --generated-at supplies caller_supplied. Explicit timestamps normalize to UTC milliseconds and reject leap seconds. Authorization always uses actual runtime time.
- Authority/accountant/SAF-T server exports audit CompanyExport under current human authority, verify originals and return tar. Original CLI handoff/SAF-T and MCP authority then publish verified files in the invoking process filesystem, fresh package root/no overwrite. A local publication failure can follow server audit commit; never automatically re-export.
- Annual CLI --ixbrl-out verifies server XHTML hash and creates a new 0600 file; no overwrite, failed publication cleanup is attempted. JSON and human modes have different streams; annual business-false JSON exits1/stdout while human exits1/stderr.
- CLI global local_binding consumes split/attached --local-profile before dispatch, installs exact-origin private Unix transport, no TCP fallback. It can normalize company/environment selectors before leaf parsing. Leaf flags are not the public argv grammar.
- Generic remote recognizes flags before earlier family guards. Source/report/format is refused before selected native report leaves; service authentication allowlist excludes native vat, annual-report, EU/OSS report. Compliance branches earlier and has its own allowed flags including explicit --out.
- Regulatory coverage/citations use embedded static sources, not company compliance or legal approval. Company compliance owner branch may read retained backup files and verify signatures; nonowner backup details remain unavailable. No remote provider verification is implied.

## Checks actually run

- `python3 /tmp/r56-677-oracle.py vat_annual_regulatory`: FAIL, `AssertionError: Coverage/duplicates/extra/gaps` (expected: no operation rows were published).
- `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"`: completed; 34,009 nodes and 107,681 edges, generated output outside owned code.
- `git status --short`: only the four owned flat files are untracked/changed.

Existing accepted runtime proofs were not replayed. Prior partitions and all global acceptance scope remain unchanged.
