# Trial-balance wire baseline

This R01/R08 increment freezes executable examples and field schemas for one capability. It extends the first Rust read-only acceptance harness; it does not implement an HTTP/MCP adapter or complete either issue. The original inventory.json is preserved. Run `bun run scripts/rust/verify-report-contracts.ts`; `bun run rust:verify` includes it.

The checked-in trial-balance-wire.json contains named source hashes, generated JSON Schemas, raw CLI streams, HTTP bodies/headers and independently checked synthetic receipts. HEAD identifies the capture context; the source hashes bind the actual named worktree bytes. They are anchor evidence, not an exhaustive transitive dependency closure. Regenerate to a temporary file and review deltas before replacing this snapshot.

## Operation ownership and contracts

| Operation | Inputs and context | Output and errors | Owner |
| --- | --- | --- | --- |
| Legacy CLI `report trial-balance` | Local company path or registered slug/environment resolution; inclusive from/to dates; global JSON/human flags | Direct numeric-money TrialBalanceReport. JSON is pretty printed plus LF on stdout. Successful exit 0; business refusal retains all report fields, zero rows/totals/counts, `ok:false`, nonempty errors, exit 1. Usage errors are plain stderr with empty stdout, exit 2. Uncaught storage exceptions are not promised a typed envelope. | R44 interface; R32 domain; R11 first slice |
| HTTP `GET /api/companies/:slug/trial-balance` | Workspace company slug; optional four-digit `year`. Empty/absent year defaults to latest live year, then available archive, then current UTC year. Router owns authentication and company authorization. | `{ok:true,trialBalance:{...}}`; numeric-money `rows`, company/fiscal-year/source metadata. No core counts, normalBalance, appliedRules or errors. Expected errors use safe `{ok:false,errors,code,subcode?}` with HTTP status. Unhandled internal errors map to generic 500. | R42 interface; R32 domain |
| HTTP `GET /api/companies/:slug/trial-balance/export` | Same selection, export permission, optional format: default csv, case-insensitive csv/pdf | CSV UTF-8 BOM, semicolon delimiter, Danish decimal comma, CRLF, quoted fields and metadata; PDF supported. Attachment filename, private/no-store and nosniff headers. Invalid format 400. | R42 interface; R32 domain |
| MCP | No corresponding registered trial-balance tool. The registry and docs explicitly classify the report command family as CLI-only. | An eventual MCP report tool is additive, not preservation of an existing tool. Registry count/name check is paired with explicit source documentation; it is not a claim to semantic equivalence analysis of every differently named tool. | R45 |
| Experimental Rust CLI `trial-balance` | Explicit company path and canonical inclusive from/to dates; three distinct flags, each with separate value. No implicit company or automatic retry. | New `{ok:true,format:"rentemester-trial-balance/1",data}` with exact two-decimal string money. Operational refusal is JSON stdout, exit 1; usage refusal JSON stdout, exit 2. No stderr for tested normal refusals. | R11; later R44 integration |

Schemas in the capture specify field names, scalar types, array/object structure, optional fields, nullability and casing. Business invariants are separate executable assertions: correct independently calculated totals/counts, ledger/company selection, status and error messages, legacy/Rust data equality for supported fixtures. A shape schema alone cannot establish report correctness.

The legacy CLI accepts attached flags and duplicate flags (last value wins). Its date helper validates trimmed strings but passes raw values into comparisons/SQL. The versioned Rust CLI deliberately has stricter input. This is an explicit version boundary; it must not silently replace the legacy command or numeric HTTP wire.

## Read and retry effects

The underlying report calculation queries all journal statuses in the inclusive transaction-date interval, rounds each line to øre, retains touched zero-net accounts, uses debit-minus-credit for every account type and sorts by JS UTF-16 account-number order. Existing read-only Rust tests cover rounding, ordering, reversals, malformed storage, source capture and concurrent writers.

Legacy CLI/HTTP adapters open writable SQLite connections and call migrate. They can create/normalize/backfill data and recover/update sidecars; their registry read classification is not a filesystem immutability guarantee. No automatic application retry loop is implemented here. A repeated read can observe newer state; SQLite busy waiting is distinct from replaying an application operation. Hosted permission/session/expiry behavior needs separate qualification before server cutover.

The Rust reader's private snapshot and no-source-write guarantee remains governed by ../read-only/CONTRACT.md. This harness creates three synthetic companies in a seed directory, closes its connections and collects deferred legacy statements, then freezes a separate copy as the oracle. Every CLI/HTTP/Rust execution receives another disposable copy. File-name/hash inventories before and after all checks must match on the frozen oracle. The seed and all copies are removed on success/failure. Nothing opens a real company.

## Fixed oracles and recorded defects

Synthetic Alpha posts 12.34 debit/credit, Beta 98.76, and a July-start span-labelled company posts 5.67 in June 2026. Each also has a separate 2024 archive with 7.89 signed balances. The raw rows are synthetic reporting fixtures, not evidence that the normal posting validation/audit pipeline was exercised. Archive selection is tested separately from live ledger parity.

- Legacy JSON success, `--json` alias, pipe default, human success, full invalid/inverted-period refusal, exact usage stderr, duplicate/attached syntax and missing required date are checked.
- HTTP live/default/empty/archive/two-company selection, complete field shape, nullable CVR/source, 400/401/404/405, export format negotiation and safe error bodies are checked through the actual router in-process. This is not a socket, reverse-proxy, hosted-membership or browser test.
- CSV has a full independent fixed-clock byte oracle: every metadata/header/data/total row, omitted CVR, BOM, escaped quotes, decimal comma and final CRLF. HTTP comparison normalizes only the explicitly identified Udtrukket date line. Dropped header/total negative controls must fail. PDF coverage here checks format signature and headers only; rendered financial values need the existing PDF suite and eventual Rust parity.
- Comparator/schema negative controls must reject wrong totals, omitted accounts, changed casing, nullable money and string money on the legacy numeric wire. Removing a business/authorization gate or injecting a partial commit is still broader R08 work.

The following defects are reproduced as observations, with corrected expectations recorded rather than silently copied into the target implementation:

| Observation | Corrected target expectation | Owning delivery |
| --- | --- | --- |
| Explicit `year=0001` becomes `1-01-01`; core rejects the invalid date but HTTP returns `ok:true` with empty data and `balanced:false`. | Keep canonical padding or explicitly refuse; never present a hidden core refusal as successful empty data. | R32/R42 |
| Default span year `2025-26` has fiscal bounds 2025-07-01 through 2026-06-30, yet trial balance uses calendar 2025 and omits the 5.67 June posting. | Resolve selected fiscal bounds and include the posting; refine the cross-surface correction contract before implementation/cutover. | R32/R42/R46 |
| CSV's Udtrukket metadata defaults to the actual UTC wall-clock date, despite a source comment suggesting byte determinism from ledger/year alone. | Treat generation date as an explicit input to deterministic acceptance. Do not normalize amounts/rows away. | R32 |

These observations do not authorize production changes or close existing defects. The implementation issue already owns their correction; no duplicate enhancement is created.

## Remaining baseline coverage

R01 remains open. This is one capability among 265 CLI keys, 214 MCP tools and 208 HTTP routes in the draft capture. Complete every operation's input/result/refusal/protocol schemas, domain ownership, retry identity, oracle and failure cases; cover providers/jobs/screens/release contracts and reconcile open enhancements. For this report, remaining cases include environment/slug resolution, TTY behavior, human failure output, all malformed-storage errors, full archived revisions/unmapped account types/live-over-archive precedence, custom fiscal labels, full hosted company permission denial and transport failure. R08's omitted-gate/partial-commit/transport-loss qualification remains outstanding. All broad epic issues remain open.
