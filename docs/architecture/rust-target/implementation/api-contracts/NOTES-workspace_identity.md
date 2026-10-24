# Workspace identity HTTP declarations — #677 bounded remainder

## Scope and evidence

Exactly 13 selected operations, with no company-create operation, CLI/MCP capture, broader registry claim, or issue closure. Current source is `da30984a56fc08877f0dbdaba799527b05447c66`. The four owned artifacts are standalone declarations; provenance hashes bind current native bytes. Graphify was queried first; source constructors, adapters and recovery paths were then read directly. The graph's broad workspace query surfaced legacy nodes, which were not substituted for Rust wire producers.

Previously accepted protocol and #712 actual HTTPS/OSKeychain evidence remains retained as supplied by the coordinator. This worker does not rerun effects, server, browser, Cargo or product tests. The supplied Python oracle checks coverage, shape structure, local references and source hashes, not behavioral acceptance. Main retains actual constructor/recovery review and full original #677 scope (including original 55/J01–J07).

## Reading the declarations

`success`, `refusal`, and `parseProtocol` identify exact status/media/body variants. JSON schemas describe the producer body, not the shared React cleanup envelope. `producerSchema` on HEAD refusal identifies the underlying handler constructor, **not a wire payload**. Source-backed `representation:none` describes actual endpoint bodylessness. The outer negotiation middleware can return its own 409 JSON before the endpoint HEAD response stripping; it is deliberately separate from endpoint refusals. Tuple schemas use one prefix item with min/max length one; this keeps arrays closed without boolean schemas unsupported by the existing structural oracle.

The forwarded control-failure schema preserves the adapter's actual dynamic error-code string and single `errors=[code]` relationship rather than inventing a finite exhaustive enum or replacing every error with `request_refused`. Cross-field equality and UTF-8-byte/UTF-16-unit limits are stated explicitly where JSON Schema alone cannot encode them. No arbitrary JSON escape hatch is needed: these producers project fixed fields, not user-provided arbitrary objects.

`createdAt` on active human company profiles is SQL TEXT returned verbatim by the profile producer, not a timestamp parsed by this adapter. Invitation creation/expiry use the native UTC millisecond formatter. Null `userId` is always present in unaccepted invitation responses; role fields on change outputs are explicitly null on removal. Optional change input strings accept omission or null where the action requires `None`; empty strings are not equivalent to null. The valid semantic input schemas constrain roles/actions, while malformed or disallowed values may first deserialize and then fail at the worker/native layer.

## Handler differences that matter

* `/api/me` belongs to optional native context callbacks. Human/service callback absence gives GET 404. Its explicitly installed HEAD closure gives 405 with `Allow: GET`, independent of cookie, callback presence, body, query and native peer rate. It is not a positive GET-shaped response.
* Product GET routes use Axum's implicit HEAD handling. Workspace reads perform the read command and return no wire payload. Human companies HEAD still performs current context and live-ledger profile enrichment before stripping the body. Service companies HEAD reaches `read_headers` and refuses 400 `invalid_query`; no service success is claimed for HEAD.
* Product human reads ignore request bodies and content headers; they reject any query. Native GET me requires an empty bounded body and refuses content encoding. Service GET companies additionally disallows transfer/content encoding and idempotency headers and requires absent/zero length plus an actually empty body.
* Native me has its own peer admission/rate limit and four-worker pool. Workspace/product routes have an eight-worker pool and no corresponding HTTP peer-rate limiter in their adapter. Importing native auth rate semantics into these product routes would be false.
* Native errors are `{code:"request_refused"}`, `INVALID_CREDENTIALS`, or `AUTH_UNCONFIRMED` as selected by branch. Product errors are `{ok:false,code,errors:[code]}`. Workspace post-result responses add `Referrer-Policy: no-referrer`; early proof/parse/permit refusals do not. None of these operations returns or clears a cookie. A client-memory cookie clear is not durable credential revocation.
* Human company rows enrich active entries with ledger profile name/createdAt under fresh CompanyRead authority. Archived entries retain catalog name and omit createdAt. Service company rows retain catalog metadata and never acquire profile enrichment. Service me/companies context can charge key usage; these are not automatically retryable safe reads.

## Ordering and current authority

Context companies retain admitted catalog order and latest membership grants; archived entries remain visible. No pagination/query normalization is offered. Invitation IDs are read in descending issued-event ID order, bounded at 10001 (over 10000 refuses) **before** company-owner visibility filtering. Latest status is projected; expiry is not a list filter. Member users order by `lower(email),id`, with the same global 10000 guard before active-access filtering. Membership arrays retain catalog order and include only nonarchived companies where the caller is currently owner. No total/count envelope is fabricated for these workspace arrays.

Workspace members/invitations permissions require `workspace_owner`; ordinary `member` is insufficient. Native authorization rechecks verified email, verified MFA, current session and current access under the held writer. Issue and company grant/revoke require current owner authority on a registered nonarchived company. Although storage permits archived revocation, the application admin gate does not; the HTTP declaration preserves that restriction. Last effective ready-owner protections use current state, including all catalog company scopes for workspace owner removal/demotion. Explicit same-state changes are storage no-ops.

## Unknown outcomes and secret policy

Invitation issue commits an intent before mail handoff and commits delivery status separately. Each issue creates a new ID/token, not a deduplicated email request. A 503/timeout cannot establish nonissuance or nondelivery; read current visible invitations and correlate the stable invitation ID before cancel/restart. `delivery_failed` cannot currently transition to cancelled. Mail confirmation is handoff, not recipient receipt. Tokens/digests are never returned in these HTTP artifacts or examples.

Claim inspects the token, validates/hashes the password, creates or retains the email identity, issues verification mail, then re-inspects and accepts. Existing identity credentials/name are retained, never reset by invitation possession. Failures can leave an identity or mail without accepted invitation state; no blind replay is safe. Accepted, cancelled, expired and otherwise non-delivery-confirmed invitations refuse reuse. Success never mints a session, asserts email verification/MFA, or returns a password/token. `accessReady` and `nextStep` describe subsequent human steps, not a reusable authorization grant. Current get-session/me can establish an existing caller session only, not prove that claim produced one. Authorized admin invitation readback is the completion authority keyed by invitation ID, subject to visibility limitations.

Access disable durably invalidates/deletes target sessions on an actual state change. Company revocation changes company membership, not session credentials. Readback absence is not sufficient proof when users/companies are filtered, archived, disabled or caller scope has changed. Unknown writes require current-authority readback and deliberate recovery; no endpoint supports a client idempotency key, key TTL or tombstone promise. Retry matrices use null key scope/TTL/tombstone and explain actual natural no-op/one-time behavior rather than promising an original-key replay.

## Validation

The supplied oracle is run after all four files exist. AST extraction uses the isolated `$SGD_BUILD_DIR/graphify` output, not tracked graph files. Neither check replaces main's source-constructor and recovery review or prior runtime acceptance.
