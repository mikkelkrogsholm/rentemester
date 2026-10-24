# Invoice MCP exact source contracts — Work-Issue: #677

This partition owns exactly 21 MCP identities: ten native and eleven original tools.
It does not close #677 or substitute for all advertised operations, all55, J01–J07,
or final installed-desktop acceptance. Existing identity/account-security and accepted
financial/mail/invoice evidence (including #692/#709/#711) is retained, not rerun.

## Reading the artifacts

`operations-invoice_mcp.jsonl` records all eight contracts per identity. `schemas-invoice_mcp.json`
is draft2020-12, with local definitions for current input and native producer shapes;
only already accepted identity MCP protocol/SDK JSON definitions are referenced externally.
`provenance-invoice_mcp.json` binds exact current bytes by SHA256 and literal needles.
Success schemas describe complete JSON-RPC tool replies. `upstreamBody` separately
records the native HTTP JSON (PDF tools describe their SDK-synthesized body).
Discovery outputSchema's generic data object is **not** native producer evidence.
SDK accepted JSON is deliberately separated from exact native success producers.

## Boundaries that must not be conflated

* `tools::call` has an explicit `args.is_object()` guard before every selected dispatch.
  Therefore the derived native MCP argument structs cannot accept sequence arguments
  through this actual entry point. Direct native HTTP Serde bodies have no equivalent
  guard and may accept positional arrays: exact declaration orders and tuple lengths
  are recorded in parseProtocol; SDK body validation still requires object input.
* Frame JSON is parsed into Value: duplicate keys collapse last-value-wins before
  MCP validation. Native body strings are forwarded unchanged after SDK Value-based
  validation; native HTTP derived recognized duplicate fields refuse. Original structured
  arguments are reserialized after collapse; direct original HTTP recursively rejects
  duplicates through Unique. These are different boundaries, not normalization promises.
* Native success structuredContent wraps the entire HTTP response as data. Original
  tools preserve the original canonical envelope directly. Both serialize that same
  structuredContent as one compact text content item; isError rules are source-specific.
* `credit_invoice` and `invoice_credit_note` are distinct interfaces. No `invoice_create`
  or repair tool is advertised by this selected MCP implementation. Original render
  verifies an existing PDF and returns a reference/hash, never binary regeneration.
* Raw HTTP PDF is not JSON. SDK/MCP PDF uses integer bytes and MIME/signature checks.
  No local output files are created. HTTP HEAD wire bodies remain empty, even where
  a GET constructor would have produced JSON or a middleware error.
* Native invoices is fiscal-year scoped; original list/find/status/overdue is all-year
  source-validated readback. asOf changes aging only; histories remain current. Amounts
  remain JSON numbers, nullable claim principal stays null for foreign currency and
  separate DKK claims are not silently merged. Contact GDPR overlay is explicit.

## Safe recovery

No selected tool has an idempotency/replay key, expiry or tombstone promise. No automatic
retry. Unknown writes require exact authoritative invoice/document/journal/bank/evidence
reconciliation with current access and the same ledger identity. Absence under changed
access/year/source is not proof of noncommit. Partial credit can otherwise be applied twice.
A reused receipt never authorizes posting, sending or reissuing as a new operation.
Service admission can charge usage independently of business result; current authorization,
backup/transfer fence, current posting configuration and closed periods remain mandatory.
Provider uncertainty in existing PEPPOL projections remains unresolved by these reads;
none of these tools sends mail or invokes delivery providers.

## Verification

Graphify query performed first; native current source inspected directly. No Cargo,
server/browser/network, effect replay, dependency or framework change. The strict existing
Python oracle is the intended artifact check; main owns actual producer review and acceptance.
No commit, push, merge or issue-status action is performed by this worker.

Artifact check: `python3 /tmp/r46-677-oracle.py invoice_mcp` passed for all21
resolved source-bound schema contracts; semantic producer review remains with main.
Required independent code-only Graphify extraction completed in SGD_BUILD_DIR.
