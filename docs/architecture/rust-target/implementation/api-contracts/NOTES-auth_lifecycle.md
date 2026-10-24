# #677 — bounded auth lifecycle declarations

Exactly the selected 15 HTTP method/path identities. This is not full #677,
all identity routes, the advertised registry, original 55/J01–J07, CLI or MCP.
The lead retains acceptance and issue status. No production change, request
replay, dependency, build, browser, or product test framework is part of this work.
Previously accepted #712 runtime/protocol evidence remains separate evidence;
source declarations do not manufacture additional runtime success receipts.

## Current installation is not capability advertisement

`Backend::identity` constructs native recovery and verification handlers, but
sets `password_change`, `backup_management`, and `session_control` to `None`.
Their routes are registered, but return 404 `{"code":"request_refused"}` before
browser admission or parsing. The artifacts specify their actual conditional
native wire constructors without claiming they are enabled in the current
production backend. There is no missing constructor for their conditional
outputs: application/storage implement the native shapes. The concrete absent
production wiring is those three optional fields, not permission to add code.
HEAD closures still run independently and return 405 with `Allow: GET`, with
Axum suppressing entity bytes; they neither authenticate nor execute GET.
Outer protocol negotiation can reject first with its distinct 409 envelope.
Disconnected/unregistered API fallback envelopes are installation context,
not invented native-handler positive responses.

## Exactness notes for review

* Password change returns `{status:true,token:null,user:<SessionUser>}`. The
  storage producer serializes the exact `session_view::SessionUser`; HTTP
  accepting an internal JSON object is not evidence of arbitrary public user
  fields. Native success contains no clear password or JSON bearer. Rotation
  deletes all old sessions and appends one browser cookie; nonrotation does not.
* Session list is a bare array, not `{data:...}` or a legacy SDK envelope.
  Metadata optionals serialize explicitly as null. IDs are stable revocation
  targets, never bearer tokens. The bound is 1000 scanned rows including expired
  rows and 1 MiB serialized output; overflow refuses, not truncates. Listing
  also writes an audit record. Revocation is same-account and target IDs are
  binary compared; a missing/other-account target still acknowledges success.
* Reset-request, reset-completion, verification-request use Serde structs
  without an object-prefix guard and therefore accept positional JSON arrays
  as well as named objects. Schemas preserve this difference. Password-change,
  backup management and non-list session actions explicitly require objects.
  Unknown and duplicate struct fields fail. Present optional strings and bools
  cannot be null. Limits expressed in UTF-8 bytes or original UTF-16 code units
  are not silently recast as JSON Schema Unicode-code-point lengths.
* Request-password-reset checks any query as 403; reset-completion and human
  `gates` reject query as 400. Reset paths do not run human-only gates and do
  not inspect Authorization/service-key/cookie headers. Verification completion
  rejects those authorization headers with INVALID_CREDENTIALS. The artifacts
  do not impose another endpoint's header behavior on these handlers.
* Recovery request ACKs deliberately conceal unknown account, disabled delivery,
  native failure, commit uncertainty, delivery uncertainty and panic. Verification
  request has a 500ms floor after admitted work starts, not constant-time proof.
  There is no public delivery receipt. Cancellation does not cancel blocking work
  or release its worker permit early. Generic ACK is not proof of an email sent.
* Reset callback is read-only preview, not consumption or authority. Empty callback
  selects the native auth-error destination; missing callback query is 400.
  Syntactically valid missing/expired reset tokens redirect with INVALID_TOKEN;
  malformed path tokens return the distinct message/code body. Completion
  consumes every subject reset link and deletes every subject session atomically.
* Email verification GET is effectful. Original signed bearer is verified again
  under writer admission and matched to persisted digest, email, workspace and
  current lifecycle/credential binding. One-hour signed state is single-use.
  Callback processing precedes mutation. JSON variants and redirect error
  variants differ; redirects strip old token/error/verified parameters and do
  not inject a verification bearer. No session, MFA or membership grant occurs.
* Backup generation returns ten unique codes; view returns zero to ten remaining
  codes in stored order. Both are password-bound and post-commit disclosures;
  generation replaces the pool. Individual codes are consumed by backup login,
  not by view. No secret-valued examples are included.

## Recovery and client boundary

No selected operation supports an idempotency key or cached replay result.
Every retry object explicitly sets unsupported key scope/TTL/tombstone to null.
No unknown pending mutation can be replayed blindly. Readback is tied to current
`GET /api/auth/get-session` session.id and user.id; it is not credential success
or a durable mutation receipt. An absent/null session cannot distinguish expiry,
revocation, malformed credentials or a lost rotation cookie. Password confirmation
requires explicit sign-in with the intended password and fresh TOTP, not reposting
an old password change/reset. Revoke-all/others can affect newly created sessions
on repetition; regenerated backup codes invalidate previous disclosures. View is
an explicit password-authorized readback, not deduplicated generation recovery.
Anonymous email verification has no public email-state readback; restart/cancel
is an explicit user choice, not automatic effectful-GET retry.

The four native cookie clears are local browser expiry, not independently durable
credential revocation. Durable revocation is evidenced only by the committed
native delete. React cleanup/transport envelopes must not be inferred from raw
HTTP responses or used to normalize them silently. No legacy TypeScript response
or SDK schema substitutes for the native constructors here.

## Validation boundary

Source hashes and needles bind current files. The supplied strict oracle validates
frozen selection, structural schema/reference completeness and provenance; it
cannot prove semantic equality with Rust constructors. The lead must review that
comparison and current optional-handler installation. No runtime acceptance was
rerun and no global completion claim is made.
