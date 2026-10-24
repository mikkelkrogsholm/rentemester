# Chart / journal operation contracts — #677

Status: specified. Unresolved: []. Scope: exactly the 54 selected operation IDs in
this partition, not whole-issue acceptance. Prior 212 accepted contracts are not
modified. Whole accounting/voucher flow, all 55 families, J01–J07, final manual
installed-desktop acceptance, release, cutover and retirement remain with main.

## Method and limits

Graphify was queried first. Its returned graph was predominantly legacy
TypeScript; current Rust source was then traced from CLI `main` and global
`local_binding` / `original_args`, MCP Session and outer `tools::call`, and HTTP
protocol admission through credential proofs, current held owner admission,
application control/service fences, actual storage and engine producers, then
SDK response parsing and adapter projection. SHA-256/needle provenance binds
current source bytes. These are declarations for independent semantic review,
not runtime acceptance receipts. No Cargo, server, browser, network, provider,
posting, upload, scanner or live HEAD invocation was performed. Existing accepted
runtime/userflow proofs and seven-route HEAD fixture are not replayed.

## Surface distinctions

* HTTP native derived structures can accept full declaration-order sequences;
  MCP public arguments cannot: the outer object guard precedes every dispatch.
  Exact native JSON strings in CLI/MCP pass SDK object validation first.
* Original journal CLI `post` and `reverse` are unkeyed; original MCP has optional
  explicit keys selecting the separately advertised keyed HTTP routes. No
  recover CLI/MCP aliases exist. Original CLI `--input -` is a literal file,
  never stdin. `--format` is not accepted by this parser despite its help text.
* Correction to an initial reading: native `remote post-journal` /
  `remote reverse-journal` are reachable **without** outer `--confirm`. That flag
  is refused by the generic remote confirmation guard; confirmation and the key
  are inside the stdin envelope. No unreachable-success claim is made.
* Native journal report, original CLI bare snake_case journal list array, and
  MCP `journal_list` page are different producers. The latter removes
  `bookedDebitTotal`; the CLI requests all headers in one admitted snapshot and
  uses the server total only for human display. No client arithmetic is implied.
* `validate-journal` and `verify-journal-hashes` are experimental LOCAL-only
  physical-source reads. Preview horizon is advisory; journal hash compatibility
  is not full audit integrity or posting authority.
* Native account setup is only the account subset here. Native accounts includes
  ten resolutions (eight required plus FX roles); original accounts has exactly
  eight. Source account projections and original MCP camelCase projections differ.
  Current constructor enums must not erase historical SQL reader possibilities.
* Original account business failures retain exact 422 bodies through their SDK;
  original journal retains validated 422 and supplies transport code/rules.
  Generic native SDK discards non-2xx bodies, while HTTP200 native keyed refusal
  remains `ok:false`. Tool envelopes are not standardized across these families.

## HEAD and effects

Global protocol admission still precedes handlers. Native `/accounts` HEAD has
an early 405 `Allow: GET` before credentials/body/owner. Other owned HEAD routes
retain actual in-handler behavior: journal/report and history GET-only guards;
original accounts method mismatch behind current-authority fencing; manual
recovery HEAD enters the inventory read branch and never acknowledges. Axum
strips HEAD response bodies. No live HEAD effect probe was used.

Writes retain current principal/company/service permissions, MFA-derived session,
physical namespace admission/retirement, backup/transfer fences, current rule and
opening-history proof, source evidence, period restrictions and atomic financial
writer controls. Service quota/control metadata is distinct from the company
business transaction. Dry-run rolls back all company writer effects but is not a
blanket assertion of no control/service metadata. Absent results or changed
readback authority are not rollback evidence.

## Retry and historical values

No automatic retry is promised. Native keys and optional original MCP keys bind
ledger UUID, operation, principal kind/subject, caller key and canonical intent.
Retained outcomes expire after 30 days; durable tombstones do not grant reposting
when outcomes disappear. Imported-key reconciliation remains separate; original
keyed journal refuses restored provenance rather than silently discarding it.
Browser recovery is only opted-in current-user receipt inventory/acknowledgement,
not financial replay. Acknowledgement does not change receipt/key/journal.

Arbitrary JSON is limited to actual `Option<Value>` reversal selectors and
historical `receipts[].result` parsed from retained `outcome_json`. It is not a
top-level data escape hatch or a substitute for current producer schemas.

## Checks

The existing strict source/hash/selection oracle is the bounded check for this
partition. Semantic integration and acceptance belong to main. Graphify AST
extraction is written only to the independent build directory, outside these
four owned flat files.

Lead review corrections: native keyed known-business refusals retain `{ok:false,data:{ok:false,code,errors}}` and `isError:true`, distinct from SDK errors. Original MCP post/dry-run calendar strings are nonempty parser input and become server business validation. Original CLI global preprocessing emits empty stdout and text stderr exit2. RPC code/message pairs enumerate actual Session/framing and selected leaf errors; argv diagnostics are not RPC. No runtime effects or negative probes were executed.
