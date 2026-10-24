# Native remote identity CLI contracts — Work-Issue: #677

## Boundary and evidence

Eight exact `remote` leaves, not command families: login, me, companies, service-principals, create-service-principal, rotate-service-principal, revoke-service-principal, recover-service-principal. All rows are CLI (no MCP aliases). Advertisement and dispatch in current native Rust source are authoritative; legacy R01 inventories omit additions. Login is one command with `--factor totp|backup`, not two commands. There are no advertised invitation, reset, enrollment, session-revoke or signout counterparts in this selection.

Read AGENTS.md and queried Graphify before inspecting current source. Provenance binds full current file bytes with SHA-256 and literal needles. Accepted HTTP schemas are referenced only for genuinely identical **inner native payloads**; CLI envelopes are owned here. No production code, dependency, framework, inventory, queue or other partition is changed. This is source documentation, not a replay of authentication or other provider/business effects.

Retain accepted #712 shared identity flow and production-account-security R44 proof `32a36073b8c1dc0567113cba86c0fd02de3191e14be483bc6edd615de077efdb`. This partition does not close #677 or diminish epic #672, all 55 original issues/J01–J07, or every advertised HTTP/CLI/MCP scope. Main owns acceptance.

## Invocation versus returned value versus process output

Input schemas encode normalized flag maps and logical stdin, not an invented JSON request envelope. CLI argv is tokenized by remote.rs; flags may surround the single command. Value flags cannot have empty values or next values starting `--`; duplicate flags, extra positional words and unknown flags refuse. No `--json`, `--flag=value`, or `--` support is added. Sole `remote --help` is discovery, not an invocation of a selected leaf. Native remote arguments bypass original-dialect formatting. Despite a broad help sentence, this remote parser does not accept `--local-profile`.

Successful remote handlers return `Ok(Value)`. Main serializes that value as one stdout JSON line plus LF, with empty stderr, exit 1 exactly when outer boolean ok is false, otherwise 0. Failures return `Err((exit, Error))`; main constructs `{ok:false,code,errors:[message]}` on stdout, not stderr. These are not HTTP envelopes. Main's original-dialect preprocessing has separate stderr usage failures, but explicit native remote is opaque to that migration/formatting path.

The SDK accepts any syntactically valid JSON on successful HTTP status. Native producer schemas describe expected exact receipts; `SdkAcceptedOutput` separately captures SDK tolerance without pretending it validates those receipts. Outer ok copies inner data.ok only if boolean, otherwise defaults true. Arbitrary JSON is justified only at that SDK response-value boundary; unknown successful fields are retained, not stripped. Normal native success payloads remain exact closed objects.

Non2xx server bodies are discarded. CLI server_refused messages contain the HTTP status only; native error codes/envelopes do not leak through as CLI data. Reads classify transport/chunk failure as transport, oversize/invalid JSON as protocol, any non2xx including 5xx as server_refused. Body-bearing writes classify dispatch/chunk failure, 5xx, oversize or invalid JSON as outcome_unknown. Every send first performs credential-free manifest admission; a preflight failure is protocol before dependent dispatch. There are no automatic retries.

## Login and private material

Login is the existing email/password plus MFA ceremony, not a service-only API-key ceremony. Existing cookie/service-key flags are refused. Email has SDK byte bounds but no CLI email syntax validation. `stdin` is **plaintext** nonterminal input, not a JSON array: password then factor line; LF mandatory, one preceding CR removed, valid UTF-8, 1–4096 bytes remaining per line. A 4099-byte bounded line read rejects missing LF/oversize. Terminal echo is refused, but the code does not prove a pipe is private or encrypted. The factor line is consumed only after the first response establishes a pending challenge. Extra trailing input is not inspected.

Enrollment-required returns `Ok` with a special ok:false value and exit 1, without reading factor or writing a file; enrollment remains in the UI. Auth response processing differs from ordinary request: 65536-byte cap and origin-bound required cookie attributes; malformed/missing issued cookie, malformed auth response, auth 5xx or lost body are unknown, not a generic protocol receipt. Cookies use HTTPS secure-name selection; domain cookies and missing Path=/ or HttpOnly (and Secure for HTTPS) refuse. No credentials are printed.

Destination symlink_metadata must report absence before dispatch. After authentication, create_new Unix0600 plus write_all/sync_all never overwrites an existing file. The parent must already be private; this function does not inspect its permissions. Non-Unix persistence refuses. A post-auth local file failure is invalid_input/exit2 even though the session may already exist and output may be incomplete. Missing output is not proof the server issued no session. Library pending challenge retry is explicit/in-memory; CLI exposes no persistent resume handle.

Cookie-file and service-key-file policies are intentionally distinct. Cookie files follow opened-descriptor regular-file/size/private-mode checks, not an explicit symlink/hardlink prohibition. Service-key files reject symlinks, hardlinks and replacement identity races on Unix, allow exactly one final LF/CRLF and enforce stricter trimming. Neither file error prints the path or secret. Issuance outputs a one-time secret in JSON stdout: callers must route stdout to private storage; no secret examples are included here.

## Current context and lifecycle

Me and companies permit either a human cookie or service key, never both; no environment actor or credential fallback. Me returns the same context schema for both: user, workspaceRole and catalog-ordered granted companies. Companies has two producer variants: human active companies enrich name/createdAt from admitted ledger profile, archived entries retain catalog context; service companies retain catalog context without profile enrichment. Service context can charge current key usage/quota; describing these as read-only does not promise zero admission bookkeeping. These selectors cannot grant future company authority. Generic recognized but ignored selectors are recorded in input; they do not implement filters/pagination.

Service-principals list is current-MFA human workspace owner only. Mutations, including **recover**, require fresh owner MFA, outer `--confirm` and native inner `confirm:true`, with exact <=8192-byte stdin JSON. Service credentials/company/grants/permissions/preview selectors refuse. SDK validates shape and byte/scalar constraints, forwards original bytes once; duplicate JSON keys can collapse during local Value validation but strict server Wire rejects duplicate recognized keys. Optional means absent, not null. Expiry is not injected by CLI (native default 7776000 seconds). No revision/If-Match token is introduced.

List orders principals by created_at then binary user_id; credentials by binary id. Bounds refuse rather than truncate. enabled:null counts active; this is not an effective expiry/grant assertion. Lists contain no plaintext secrets or operation IDs.

## Reconciliation, not generic retry

All rows explicitly record automaticRetry:false, key.supported:false, scope/ttl/tombstone:null. Caller operationId is correlation/collision refusal for issuance/revoke and mandatory for recover, not an idempotent receipt replay key. Existing operationId reuse refuses 409; changing it after unknown issuance is not reconciliation. Preserve caller IDs. Generated IDs are unavailable if the original response was lost; list/name matching cannot reconstruct them.

Recover's exact four-way receipt union is retained beneath the CLI envelope: completed revoke with null replacement; completed create/rotate with replacement; recovered cleanup with nullable original credential and null replacement; recent recovering with recovered:false and null replacement. Completed/recovered terminal histories return their current known result; recent recovering (<30000ms) does not clean up yet. Nonterminal legacy create/rotate recovery disables marker-bound keys and appends failed/recovered; revoke recovery disables its target and completes. Missing history refuses, not a fabricated not-found success. No reconcile alias or external provider progress promise is introduced.

Current authority/workspace fences still apply to recovery; refused or unavailable snapshots cannot prove absence of a prior commit. Native issuance/hash/key/history changes share one transaction, not an external provider retry system. Recover never rediscloses a lost secret; list cannot recover it either. After authoritative reconciliation an explicit fresh rotation/revocation of known identities may be needed. Unknown rotation does not establish the old credential remains usable.

## Validation

Use only the existing Python oracle for this documentation partition. It checks exact selection, current source hashes, resolved schema references and the eight contract fields, **not semantic truth**. Main must review native behavior. Required Graphify AST extraction writes only to the independently assigned `$SGD_BUILD_DIR/graphify`. No Cargo/build/browser/network tests or business mutations are part of this delivery.

Observed checks: `python3 /tmp/r45-677-oracle.py identity_cli` passed for all eight exact selected rows (resolved source-bound schemas; semantic review still required). `git diff --check` produced no errors. `graphify extract . --code-only --no-cluster --out "$SGD_BUILD_DIR/graphify"` completed in the independent worker output: 34008 nodes, 107676 edges, no clustering. Graphify reported 228 code files yielding no symbols; this was a warning, not a contract acceptance claim.
