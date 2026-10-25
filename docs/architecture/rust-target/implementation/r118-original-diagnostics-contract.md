# Original company diagnostics — #717

Full mission remains all55 children/epic672, one Rust motor/server and shared
React web/Tauri UI; final installed desktop acceptance last. R117 effects and
failed setup receipts remain. This is one necessary original administration
flow, not whole717 closure. No new infrastructure, dependencies or runtime setup.

## Frozen wire and source behavior

Two new read-only routes on the existing admitted product router:

- `GET /api/companies/{slug}/retention-status[?asOf=STRING]` returns
  `{ok:true,retentionStatus:CORE}`. CORE is the ORIGINAL retention result:
  `{ok,asOf,appliedRules,rows,errors}`, rows contain table/total/expired/nextExpiry/
  oldestExpired. Reuse existing `retention::read` and its report; project its
  original known fields, with no new arithmetic or backfill. Missing date uses
  trusted server date. Explicit invalid/empty date gives original business
  `{ok:false,asOf,appliedRules:["DK-BOOKKEEPING-RETENTION-001"],rows:[],errors:["asOf must be YYYY-MM-DD"]}`
  under HTTP200, after current authorization. `asOf` is any original string in
  MCP grammar; its calendar validity is a business result. Duplicate/unknown
  query fields and any body/authority override refuse. Business date NEVER
  replaces session/expiry/permission clock. Leave `Owner::gdpr` trusted-today
  erasure guard and existing native retention/GDPR routes unchanged.
- `GET /api/companies/{slug}/system-healthcheck` returns
  `{ok:true,healthcheck:CORE}` even when diagnostic CORE.ok is false. CORE is
  existing `rentemester_storage::healthcheck::inspect_company` output including
  checks/missing/schema_outdated/schema and errors. No query/body/path selector.
  Missing/corrupt SECONDARY registered company must remain diagnosable without
  opening its ledger. Health is not aggregate workspace health.

Both routes require current ordinary human MFA/session or actual scoped service
credential with WorkspaceRead and selected CompanyRead. Mixed/stale/revoked/wrong
workspace/company credentials refuse; caller metadata never supplies authority.
Hold the current control revocation fence through the actual read. Service
quota/rate/usage semantics stay genuine and atomic; do not disguise a service
key as cookie or charge twice. Host physical namespace/transfer admission stays.

Health must retain current healthy admitted anchor: HTTP takes existing
`state.native.owner.lock()` and does NOT call `lock_company(target)` (which opens
the target). A fixed Owner method uses `catalog()` and `control_auth` plus a
new application control-only diagnostic action patterned on `parties::execute_scoped`:
validate cookie/workspace/catalog, IMMEDIATE control tx, current session and
session-control authorization, registered `workspace_members::company(...,true)`,
current human roles and access helpers, then fixed host callback on
`self.root.join(validated_registered_slug)` while fence held; rollback/readiness.
For service use existing `service_context::observe`/charge/current role/scope
primitives under same held control tx and ordinary typed failure/commit handling.
No target company SQL connection or path from HTTP. Existing inspector already
uses protected private snapshots. Do not weaken NamespaceGuard, observe/admit or
repair/start a replacement owner. A broken/retired ANCHOR remains a typed refusal;
offline original CLI remains the diagnostic alternative for that boundary.

The read-specific retention application/host method can use existing paired
human/service CompanyRead fence and existing source retention producer. It must
not mutate retention deadlines or reuse the trusted-date GDPR erasure method.

## SDK/MCP boundary

SDK new focused module `diagnostics_original118.rs` exposes:

```
ApiClient::retention_status_original118(company: &str, as_of: Option<&str>) -> Result<Value, Error>
ApiClient::system_healthcheck_original118(company: &str) -> Result<Value, Error>
```

Each returns the validated CORE object from its named HTTP field. Use existing
bounded GET/negotiation/explicit configured transport/current credentials, no
automatic retries. Do not turn missing/wrong/malformed HTTP envelopes into empty
success. Server owns diagnostics, dates, retention arithmetic and authority.
Company is explicit slug, or exactly one direct company directory under a
freshly verified explicit local runtime root as R116 workbench; reject traversal,
nested/wrong/unverified roots before business HTTP. No local ledger reads in SDK.

New MCP `retention_status` and `system_healthcheck` have their EXACT original
flat schemas/title/annotations from `src/mcp/tools/retention.ts` and
`src/mcp/tools/system/healthcheck.ts`: company nonempty string; retention optional
asOf string; no extra fields/confirm/actor. Optional input date is not RPC-pattern
validated. ReadOnly/idempotent true, destructive/openWorld false. Existing native
`retention` and aggregate health remain unchanged.

Original malformed tool arguments return SDK-compatible text-only isError result
(like R116 company/workbench), not a fabricated business envelope. Dispatch/frame
errors retain existing protocol behavior. Retention uses exact `wrapCoreResult`:
lift ok/errors/appliedRules, success data is remaining `{asOf,rows}`, failure has
no data; no invented data.report wrapper. Health uses original success/error-with-
data behavior: outer errors from CORE.errors, data retains CORE.ok/checks/missing/
schema_outdated/schema, omitting CORE.errors. Diagnostic false keeps data and
isError:true. Text JSON and structuredContent must agree. Real transport/authority
refusals remain tool failures, no successful healthy/default fallback.

## Disjoint ownership and acceptance

You are not alone. Preserve others' edits; no nested agents or issue closure.

`backend118` owns ONLY new application/host/server diagnostics modules and their
test modules, application/host lib declarations, product_http.rs route/module
registration, protocol/http-v1.json, server/src/protocol.rs generator output,
and client/tests/common/manifest.json. Run existing protocol generator, and copy
the generated full manifest to existing client fixture so new SDK routes truly
negotiate. Existing HTTP/business contracts stay unchanged. Add actual tests with
prefix `original_diagnostics118_` for normal/dated/invalid retention, no mutation,
cookie/service current role/revocation/company scope, broken secondary registered
health data, missing/corrupt/symlink schema, unregistered paths and anchor refusal.

`adapters118` owns ONLY client/src/diagnostics_original118.rs, client/src/lib.rs
module declaration, client/tests/diagnostics_original118.rs, MCP/src/
diagnostics_original118.rs, MCP/src/main.rs module declaration, MCP/src/tools.rs
registry/dispatch binding, MCP/tests/stdio.rs module/count binding, and new
MCP/tests/stdio/diagnostics_original118.rs. Add focused actual SDK and prefixed MCP
cases for full original schema/envelope/hints, dates, diagnostic-false data,
malformed responses/arguments, no-network selectors, current refusals and no
retry. Use existing `tests/common` TCP/Unix discovery fixtures; raw no-network
listeners stay raw. Expected actual tool count changes434→436, no stale370 values.

Workers run syntax checks and `graphify extract . --code-only --no-cluster --out
"$SGD_BUILD_DIR/graphify"`, one commit Work-Issue:#717. No cold Cargo builds or
package installs, provider/browser/financial effects. Lead reviews real diffs,
integrates through SGD and executes exact new app/host/server/SDK/MCP tests in
guarded warm target. Lead must confirm nonzero new tests and actual catalogue,
refresh source-bound typed contracts for every actual new route/tool before
external publication; no false full717/coverage claim. At15min growing setup or
2setup failures diagnose/reuse proof and continue independent product work.
