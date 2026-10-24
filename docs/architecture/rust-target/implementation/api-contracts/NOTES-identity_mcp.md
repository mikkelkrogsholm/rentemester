# #677 — selected native identity MCP contracts

This partition specifies exactly `me`, `companies`, `service_principals`,
`create_service_principal`, `rotate_service_principal`,
`revoke_service_principal`, and `recover_service_principal` as native MCP tools.
Current source is `5de3f7c9464b12fb86a39e608e3d9a89e8d56704`.
Native advertisement and dispatch, not the legacy R01 inventories, are authoritative.
The accepted #712 flow and R44 account-security proof
`32a36073b8c1dc0567113cba86c0fd02de3191e14be483bc6edd615de077efdb`
are retained, not replayed. Main owns semantic review and acceptance. This does
not close #677 or reduce epic #672, the 55 original issues/J01–J07, or global
all-advertised HTTP/CLI/MCP scope.

## Wire layers must not be collapsed

`input.advertisedInputSchema` records tools/list arguments; `actualRequest`
records JSON-RPC request parsing, and `parsedArguments` records Serde parsing.
All selected argument structs deny unknown fields. Request/params objects instead
ignore unknown fields, except presence of `task` is explicitly refused. The write
parser accepts a boolean false, while advertisement says true; false produces
an **inner SDK invalid_input result**, not an outer invalid-params response.
Body lengths and validator string lengths use UTF-8 bytes; JSON Schema maxLength
is characters. JSON Schema cannot express that byte measurement, duplicate-key
lexical rules, cross-field JSON text equality, or every Rust trim/control rule.
Those source-bound invariants accompany the schemas. The inner body is a string,
not an object and not a file path. No local file loading occurs per tool call.

Each native success has outer JSON-RPC result, then MCP `isError`, a single text
content block, and `structuredContent`. Text is compact serialization of the
same structured value. An HTTP envelope alone is never an MCP response schema.
Native HTTP producer payload references apply only to structuredContent.data.
The SDK actually accepts arbitrary well-formed JSON on successful HTTP responses;
this is separately modeled and source-justified, rather than claiming typed
response validation exists. Its boolean data.ok controls outer ok; absent or
nonboolean ok defaults true. A successful HTTP JSON ok:false is an error-marked
MCP result carrying data, not the SDK error object.

SDK transport/protocol/input errors are **inner results**. HTTP non2xx bodies are
discarded: the MCP error exposes only safe SDK message/code and, on Refused,
HTTP status. POST5xx is outcome_unknown; GET5xx is server_refused. Invalid JSON,
response overflow and body transport failures differ between read and write.
Bare per-send manifest preflight failure is protocol before dispatch. Outer
JSON-RPC parse/request/method/params/session errors have no MCP content wrapper.
Normal errors do not terminate the process; oversized frames do, after an error
response. Setup/private-cookie/service-key/local-profile failures are stderr and
process exit2, not tool errors. Runtime/I/O failures exit1. EOF exits0. No stdout
banner or invented transport alias is part of the protocol.

## Current authority, reads and reconciliation

Lifecycle inventory is human-only current verified-MFA workspace owner. Every
lifecycle mutation, including recovery, needs fresh owner authority and both
confirmations. Service credentials cannot substitute for this human authority.
`me` and `companies` instead support human or current service context; even these
reads can consume service usage in active mode. Neither establishes authority for
later business calls. Companies enriches active human entries with ledger profile
name/createdAt but preserves archived catalog entries; service companies uses
catalog context without profile enrichment. Inventory has its own deterministic
principal/credential order and bounds, not companies pagination semantics.

No automatic write retry exists. JSON-RPC IDs prohibit reuse within a session;
that is not business deduplication. Lifecycle operationId is stable correlation
and collision refusal, not a replay cache. A lost one-time create/rotate secret
cannot be recovered from inventory or recovery. It is present in both copies of
the successful MCP receipt and must be treated as private.

Recovery uses the actual `recover_service_principal` tool, not a reconcile alias.
Its four receipt variants are the accepted native lifecycle union: completed
revoke with null replacement; completed create/rotate with replacement; recovered;
and recent recovering. Completed create/rotate never rediscloses secret. Recent
recovering under30s returns without cleanup. Legacy pending/failed or old
recovering issuance history disables marker-associated keys and records recovered;
revoke recovery disables the exact old key and records completed. Already
recovered returns recovered:true; completed normally returns false (this call can
complete revoke recovery and return true). Missing history is a refusal, not a
fabricated not-found receipt. Current authority, registration and all native
transaction/transfer guards still apply. If the operation ID was generated and
lost with the receipt, inventory supplies neither it nor a name-based replay
promise. Reconcile authoritative IDs/history before an explicitly reviewed fresh
rotation/revocation; never infer old-key usability after uncertain rotation.

## Verification boundary

Graphify query preceded source inspection. Only these four owned documentation
files are changed. No production code, framework, dependency, inventory, queue,
other partition, authentication/provider replay, Cargo/build/browser or network
operation is part of this delivery. The existing Python oracle checks frozen
selection, hashes, resolved schema refs and eight contract fields, not semantic
truth or runtime acceptance. Graphify AST extraction is directed to the independent
worker build output. No new harness is introduced and no commit is made by this
worker, following the final runner instruction.
