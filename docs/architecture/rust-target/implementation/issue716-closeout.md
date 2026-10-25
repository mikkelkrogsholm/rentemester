# #716 final wiring handoff — not whole-issue closure

Transfer parent modules, private common-server helper and exact CLI dispatch are
wired. Two SQLite count decodes now use checked signed-to-unsigned conversion;
CLI explicitly enables the existing rusqlite serialization feature used by private
transfer ledger admission. No second engine, HTTP schema route, retry or TS fallback.

The existing protocol generator added 12 actual method/path registrations. Client
manifest matches exactly. All 38 operation/schema/provenance partitions retain
prior IDs and schema names; 42 original CLI identities bring totals to **575 CLI,
701 HTTP, 448 MCP (1,724)**. All 265 original identities carry current source
metadata; new records include typed shapes, flags, stream/exit/rendering and source
constructor bindings. The unchanged strict verifier passed. Source hash refreshes
also retain earlier field-scoped arbitrary-JSON rationale; no oracle weakening.

Executed: guarded combined check and binary build; 42 focused CLI tests in nine
suites; one real host source-transfer end-to-end test (its private child is normally
ignored); five server #716 tests; eight actual CLI-to-server synthetic first-use,
schema and owner-bootstrap/reconciliation checks. Syntax716 and Graphify extraction
ran. Paths and binary hashes are in `issue716-evidence.json`.

The whole CLI command **failed**: 51 unit tests passed, three failed before the
integration suites. Two unowned in-module fixtures omit current passive protocol
negotiation; the third requires case-distinct files on a case-insensitive host.
Their output is retained, not relabelled PASS. Owned focused fixtures were corrected
to canonical temporary paths and the actual selector-consuming SDK contract; no
product permission/file guard was relaxed. Earlier rejected outputs remain in the
worker build directory.

Full original #716 remains open pending lead actualdiff review and all four
original criteria. Preserve all 55 epic issues, accepted 223 CLI families and #717.
No live migration/cutover, Linux qualification, provider approval, final installed
desktop acceptance, GitHub closure, publication or commit is claimed here.
