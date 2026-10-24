# R05: HTTP protocol major 1

This is the major-1 compatibility policy for issue #677. The complete source-reviewed contract set contains1,490 advertised identities:639 HTTP,464 CLI and387 MCP across38 finite partitions. Each `operations-*.jsonl` row binds its exact input, success, refusal, parse/protocol, binary, selector/order, authority/revision/confirmation and operation-specific retry contract to current source hashes; companion `schemas-*.json`, `provenance-*.json` and `NOTES-*.md` retain shapes and semantic review. These transport contracts do not claim the entire Rust migration or external provider approval.

## Public metadata and current access

`GET /api/protocol` is passive public metadata. Clients fetch it without cookies, service credentials or authorization headers, using the same configured transport as subsequent calls. Rust clients retain the verified Unix socket when running locally. Metadata does not open a workspace, authenticate a user or grant permission. Every dependent operation still runs its current session, company-access, backup and accounting checks.

The manifest declares `rentemester-http-protocol/1`, `protocolMajor: 1`, an exact `manifestId`, supported `wireProfiles` and static `operations` containing method, parameterized path and `METHOD path` capability. Route parameters match one nonempty URI segment. Encoded separators remain refused for company slugs and ordinary parameters. The explicitly matched workspace-inbox source/sourceId parameter alone preserves an encoded slash as an opaque, bounded database identifier; backslash, controls, malformed encoding and traversal remain refused. Unknown methods/routes/profiles, incompatible major versions, malformed or oversized metadata and unavailable metadata refuse the dependent call before dispatch. The maximum manifest body is 1 MiB. Native metadata describes only public routes without a connected workspace; the configured runtime additionally describes its installed identity/product routes. Bun advertises its existing legacy route catalog and only `legacy-react-v1`.

The native catalog is generated from literal route registrations by `scripts/rust/protocol-manifest.py`; `--check` refuses stale output. The identity is SHA-256 of compact UTF-8 JSON with recursively sorted keys, ASCII escaping and operations sorted by `(path, method)`, excluding `manifestId`. Native GET registrations include their installed HEAD behavior, including explicit method refusals.

Existing agent-about/capabilities/workflow discovery remains passive and partial. It does not establish current authorization or complete operation-schema coverage. Offline build information is also separate from authenticated current user/workspace context.

## Explicit compatibility window

During the entire protocol-major-1 window, requests without any of the three negotiation headers use the existing unversioned compatibility path. Old React clients retain their existing numeric/string wire representations, envelopes, arrays, field casing and binary responses. Removing that path or changing the existing numeric wire contract requires a new protocol major and a documented migration.

New shared React requests use `legacy-react-v1`; new Rust SDK requests use `rust-http-v1`. They first check the requested method/path and profile, then send:

- `x-rentemester-protocol: 1`
- `x-rentemester-manifest: <manifestId>`
- `x-rentemester-wire-profile: <selected profile>`

The server checks these before dispatch. Any partial set, stale identity or incompatible value receives HTTP 409 with `{ok:false,code:"protocol_incompatible",errors:[safeMessage]}`. An identity changing between discovery and dispatch causes a refusal, never an automatic retry. Clients currently fetch metadata for each send; constructors remain network-free and no origin/session shares a mutable admission cache.

## Safe outcomes

A failed preflight means the dependent request was not sent. React returns `ApiError("protocol_incompatible", ..., 0)`; the Rust SDK returns `Error::Protocol`. After dispatch, existing operation-specific uncertain-write behavior remains binding. A lost or malformed successful write response must not be presented as a safe retry. No new idempotency key, recovery endpoint or generic original-key promise is introduced.

Revoked sessions retain the current 401 behavior. Cancelled/superseded reads retain cancellation and cannot expire a newer session through an old response. Binary payloads and success headers remain intact; malformed binary errors retain typed failure handling. A malformed PDF success still yields an unknown write outcome, and opening-input alternatives both retain strict unknown-field refusal and exact money strings.

The complete per-operation retry matrix records actual supported keys, scope, expiry/tombstones, natural deduplication, authoritative read-back and unresolved provider outcomes in each row’s `contracts.retry`. Unsupported/keyless writes retain one dispatch and explicit status/read-back/manual resolution; protocol admission does not add retry support.

## Qualified HEAD refusal correction

The public invoice configuration/submission GET-backed HEAD routes return405 with `Allow: GET` as the first handler action after outer protocol admission, before credentials, body parsing, Owner or provider dispatch. HEAD has zero response bytes, including protocol409. GET/POST controls are unchanged. The existing real-server regression passed64HEADcases over31routes with empty/invoice/confirmed-send bodies, GET/POST401 and unchanged accounting/artifact invariants. The preserved RED401 result documents the corrected dispatch defect; no provider or financial actions were replayed.

Whole #677 qualification is bound to source `909af0cfe12b16303585680928b1d0930492dce8`, SGD `rust-r59-public-head677-safety-20261024T074548.638981000` and master SHA256 `38165a775bf47126a72403e4eb8774430c412ccd0cb0cf91dca2c9e46e313626`. Retained R05 protocol receipts and all source/diff reviews remain attributed to their actual revisions. The other original issues and final installed desktop/release/cutover/retirement acceptance remain separate.
