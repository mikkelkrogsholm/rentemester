# BYO bank-roadmap integration — issue713

The Rust rewrite keeps the existing canonical CSV/account/reconciliation flow.
The original baseline has no implemented BYO provider, connection or bank-sync
producer. Epic363 remains the authoritative future bank project. This delivery
classifies its contracts; it does not implement or close that project.

`upstream.jsonl` records all11 required issue references, current states, body
hashes and ownership. `cases.jsonl` distinguishes implemented baseline, contract
preparation and pending upstream work. `source-pins.json` binds reviewed current
producers and the observed upstream snapshot. Later upstream changes require
explicit reconciliation with these body hashes and source versions before a
provider contract is advertised or implemented in Rust.

## Existing behavior and its limits

CSV original bytes, selected account/profile, per-file occurrence identities,
replay counts, physical source proof and reviewed reconciliation remain. A file
hash is provenance, not transaction identity. The documented identical-row
partial-window limitation is preserved; no general rolling CSV/provider overlap
or strong identity is claimed.643 owns that future source-neutral merge.

The required one-journal/one-bank safety is enforced by current Rust link guards
and semantic audit.642 remains open for its complete upstream schema/migration
and all-projection acceptance. Nothing rewrites historical evidence or aliases
provider transactions to artificial CSV. Existing bookkeeping-batch review and
apply are separate from future bank_sync prepare/stage/plan/apply/status.

## Verification and acceptance

The current compiled MCP fixture verifies that all10 canonical future connection/
sync names are absent and refuse before any HTTP request. Six provider/generation/
secret/plan fields are rejected on CSV input rather than silently ignored; no
secret is echoed. The existing synthetic SDK HTTP fixture verifies exact CSV
receipts, confirmation, raw bytes and typed uncertain outcomes. Four compiled
cases pass. These are CSV/boundary fake checks, not a working BYO-provider fake;
660 explicitly owns the latter.

Accepted693 bank import/matching/correction/restart/restore evidence is retained
at its recorded revisions. Its financial journeys are not repeated. Current
source review preserves those controls; bank.rs's later account-row extraction
is equivalent and covered by accepted143 account-lifecycle evidence.

All713 criteria are met by this classification, retained baseline parity and
current synthetic checks. No production code, provider account, secret store,
payment initiation, live-bank guarantee or second bank engine was added. All11
upstream issues remain open.712's final Tauri login/cancellation/OS-secret-store
acceptance remains open, as do717's full original discovery/workflow criteria.
Closing this integration issue does not close dependencies or authorize live
cutover. Manual installed desktop acceptance stays last; the full55-issue
epic and J01-J07/release/retirement gates remain.

## Existing issue ownership

| Issue | Classification | Retained boundary |
|---|---|---|
| #363 | pending-upstream | Authoritative BYO Open Banking epic; no provider implementation in frozen baseline. |
| #660 | contract-preparation | Future provider/state/role/effect contract and deterministic BYO fake belong here, not in this rewrite. |
| #661 | pending-upstream | Workspace connection/consent/session generations, historical account binding, human SCA/callback/revoke and scope. |
| #662 | pending-upstream | Provider prepare/fetch/stage, exact local plan/apply/status/recovery; not existing bookkeeping-batch operations. |
| #663 | pending-upstream | BYO onboarding/discovery/CLI/MCP/cockpit and sandbox/release acceptance; hosted/real-account provider gates remain. |
| #358 | pending-upstream | Separate operator secret mount, opaque handles, credential generations/rotation and credential-free portable restore. |
| #357 | pending-upstream | Enable Banking sandbox adapter, bounded paging, session-local identity, stable evidence and redacted errors. |
| #359 | pending-upstream | Future provider rows feed existing canonical review/matching; no parallel bank truth. |
| #356 | pending-upstream | Later Lunar evaluation; explicitly not MVP or rewrite requirement. |
| #643 | pending-upstream | Source-neutral identity/evidence observations and fail-closed ambiguous CSV/provider overlap; existing occurrence limitation is preserved explicitly. |
| #642 | pending-upstream | Required one-journal/one-bank safety already enforced/audited in Rust baseline; full upstream schema/migration/all-projection acceptance remains open. |
