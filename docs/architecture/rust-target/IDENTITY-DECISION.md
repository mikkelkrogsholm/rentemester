# Identity and authorization decision — R04

Status: selected implementable contract for [#676](https://github.com/mikkelkrogsholm/rentemester/issues/676), against `20ec05fe71bf7c28740d096f68635d26570c6884`. This record consolidates existing Rust behavior and its required gates. It does not declare the complete identity, scheduler, release or cutover issues accepted.

## Choice and transition

Choose the existing Rust-native identity/session/MFA/recovery/service-principal stack for both the shared local server and hosted server. Web and Rust/Tauri use that server's same identity path; clients do not create an alternative accounting identity or authentication authority.

| Option | Decision and reason |
| --- | --- |
| Retain Better Auth as a Bun/TypeScript identity service | Allowed only for the still-running legacy backend during transition. It preserves the old product while native flows are qualified, but retains a non-Rust backend dependency and cannot satisfy the final architecture. It is not a hidden fallback from failed native authentication. |
| Existing Rust identity implementation | Selected. Preserve the installed password/cookie formats and explicit fail-closed differences through the already implemented native password, session, enrollment, MFA, recovery, reset and service modules. Use their pinned cryptographic libraries and existing policy; no new protocol or password scheme is introduced here. |

[Password compatibility](implementation/password-crypto/CONTRACT.md), [cookie verification](implementation/session-cookie/CONTRACT.md), [login](implementation/password-login/CONTRACT.md), [MFA enrollment](implementation/mfa-enrollment/CONTRACT.md), [HTTP enrollment](implementation/http-enrollment/CONTRACT.md), [recovery](implementation/http-recovery/CONTRACT.md) and [reset](implementation/password-reset/CONTRACT.md) remain the precise component contracts. Historical credential formats that cannot be verified under the qualified profile require explicit mapping or re-enrollment, never guessed alternate normalization or a new accounting/replay scope.

**Removal gates:** R41/#712 must qualify the complete local/hosted identity and re-enrollment paths, including the still-required Tauri cancellation/OS-secret-storage boundary. R55/#727 removes the replaced TypeScript backend and its Better Auth runtime after parity, distribution and cutover gates. A temporary TS auth service, a schema-compatible cookie, or the browser's React/Better Auth client library does not prove the all-Rust-backend exit criterion. React build tooling remains permitted; native runtime authentication must not invoke Bun/Node or fall back to legacy auth.

## Threat model and retained protections

Treat request actors/roles/company selectors, expired or rotated credentials, restored sessions/consents, old worker leases and previously admitted reads as untrusted observations. Trust comes from the configured server/instance, verified exact credential and current control rows under the appropriate admission. Protect cross-company reads as well as writes; workspace ownership alone does not grant every company's permission.

Preserve verified-email and MFA readiness, current session expiry/revocation, fresh-session requirements for sensitive operations, single-use verification/reset/recovery tokens, owner-only service management and last-effective-owner protection. Human and service credentials use separate adapters; presenting the wrong or mixed mechanism never supplies fallback authority. Retain configured Host/Origin/cookie profile, CSRF/fetch checks, rate/concurrency bounds and secret-free diagnostics. Do not log passwords, service secrets, raw recovery material or prepared one-time secret receipts.

[Service lifecycle](implementation/service-lifecycle/CONTRACT.md), [current service admission](implementation/service-context/CONTRACT.md) and [company service writes](implementation/service-company/CONTRACT.md) retain their existing current-credential and stable-subject rules. Rotation preserves the accounting principal/replay scope while disabling the old credential. A historical actor, job row or stored receipt does not authorize a current caller. Replay reauthorizes before protected result disclosure.

This decision preserves last-owner protection; the separate concurrent last-owner acceptance still tracked in R13/#685 is not falsely claimed complete here. The same distinction applies to remaining R41 native credential-store qualification.

## Company commit and revocation ordering

Use the existing [control-to-company fence](implementation/control-fence/CONTRACT.md):

1. Acquire `control BEGIN IMMEDIATE` and validate the admitted control store, exact current credential/session, readiness and selected company's current permission.
2. Acquire `company BEGIN IMMEDIATE` in that fixed order. The synchronous trusted callback contains no network, await, separate connection, control mutation or transaction escape.
3. Commit the company transaction before releasing control. A revocation commits before admission and causes denial, or waits until after the admitted company commit. Do not infer ordering from a timestamp, cached role or PID.

This is not a distributed transaction or an atomic network revocation guarantee. A known pre-commit failure rolls back; rollback failure retires the connection. A commit error/lost transaction is `outcome_unknown` and requires receipt/history reconciliation rather than blind retry. If the company commit is known successful but control cleanup fails, retain the committed result and retire the unusable control connection. Do not report that case as an uncommitted rejection.

Workspace ownership/physical admission and cutover must also fence old writers; the control lock alone cannot stop an unenrolled legacy process. R12/R13 and R54 retain their corresponding runtime obligations.

## Already committed external jobs

The final current-authority check before handoff is the external dispatch admission/linearization point. It cannot make provider receipt atomic with revocation. A durable queued job, committed reservation or old consent is evidence of earlier intent, not permanent future authority. Do not hold SQLite transactions across a network call.

| Phase | Required decision | Existing implementation/evidence and remaining gate |
| --- | --- | --- |
| **Revoke before dispatch admission** | Re-observe the exact current human/service credential, grantor/membership/operation permission, source and consent/configuration generation. If revoked, cancelled, changed or unavailable, deny the new provider handoff. Regrant/rotation does not silently reactivate old consent. | Scheduler50 and hosted53 (`9d4538c9`); `hosted_recurring.rs` and its rotation/revoke/regrant tests. Every executor must join the same runtime/workspace and dispatch fences; an old reservation is insufficient. |
| **Revoke during external call** | The last admission has already authorized the attempted handoff; it cannot be retracted remotely. Preserve reservation/history and use the existing permitted terminal-recording path. If authorization/SQL fails after attempted handoff, report **unknown**, never safely-unsent. Resolve using actual provider evidence before another attempt. | `application/invoice_email.rs` admits before `transport.send`, releases transactions, and maps post-handoff finalization failure to `OutcomeUnknown`. SMTP55 proves real crash after DATA and reconciliation without resend; service102 (`ab113e47`) retains the same uncertainty boundary. No universal exactly-once guarantee or atomically closed admission-to-network gap. |
| **Lease expires** | Expiry only makes a claim stale. It does not prove that an old worker or its provider request stopped. Do not authorize a competing external effect solely because time elapsed. Require actual worker drain/fencing or the provider's qualified idempotence/reconciliation mechanism. | Service-lifecycle recovery's 30-second recovering claim explicitly requires trusted real legacy/provider drain. Native atomic control cleanup is qualified conditionally on that drain; a SQLite writer lock alone cannot fence a delayed old provider-create. Runtime/operator drain remains an enforceable activation gate. |
| **Restore/activation** | Preserve historical jobs, originals and receipts as evidence; import no active secret/session/consent authority. Reprovision identity and secrets, require current administrator reconsent, and resume/catch up only through the explicit supported operation. Preserve or verify-map stable ledger/principal/receipt scope; do not book again after a lost response. | Portable52 (`29b6ae70`) proves source-free fresh-owner recovery with inactive authority/queue and explicit reconsent/catch-up. Hosted53 proves empty service inventory, rejected old key and inactive consent. SMTP55/service102 retain unknown history without imported SMTP configuration or resend. Transfer56/99 retain one-writer activation and exact receipt mapping. |

A provider's definite rejection and its ambiguous/accepted handoff are different states. Provider-specific reconciliation can support a deliberate retry where qualified; timeout, lost reply, lease expiry, restore or a newly generated operation key is not such evidence. No delivery rollback, global archive single-use or physical offsite guarantee is added by this record.

## Acceptance basis and retained gates

The decision is bound to existing checked-in component contracts and `implementation/progress.json` evidence:

- Numeric/schema/principal compatibility: accepted R03, the existing session/password contracts, service-company stable receipts and transfer56.
- Current authorization and crash/commit ordering: control-fence source/process evidence over WAL/DELETE; current service admission and lifecycle contracts.
- Native human identity: HTTPS54 (`fd33e413`) covers ordinary MFA/invites/reset and a committed usable reset link after gateway timeout. Service53 covers create/rotate/revoke and restored authority. These are actual execution evidence, not inferred from a valid model.
- External effects: scheduler50, portable52, hosted53, SMTP55 and service102 are the phase-specific evidence above. Their original limitations remain binding; local sleep/DST acceptance and full scheduler recovery remain R35/#690 and R36/#709 work.

R04's four criteria are the explicit choice/threat/failure contract, named retained protections, named TS-removal gates and all four external-job phases. Actual implementation completeness remains with R13/R35/R41/R54/R55 and their original criteria. This decision does not waive those issues, the whole epic/J01–J07 or the final installed desktop acceptance.
