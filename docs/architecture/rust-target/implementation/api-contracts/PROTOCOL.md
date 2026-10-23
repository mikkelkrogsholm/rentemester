# R05: HTTP protocol major 1

This is the compatible client/server admission part of issue #677. Exact output schemas and operation-specific retry contracts for the complete advertised HTTP/MCP/CLI surface remain required; this delivery does not close #677.

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

The complete retry matrix must record actual supported keys, scope, expiry/tombstones, natural deduplication, authoritative read-back and unresolved provider outcomes for each operation. Unsupported/keyless writes retain one dispatch and explicit status/read-back/manual resolution; protocol admission does not add retry support.
