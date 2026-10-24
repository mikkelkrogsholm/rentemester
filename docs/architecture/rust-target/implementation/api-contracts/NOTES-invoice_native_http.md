# Native customer/invoice HTTP contracts — #677

Owns exactly 13 selected HTTP identities, not global #677 completion. Full every-advertised-operation/all55/J01–J07/final desktop gates remain; acceptance belongs to main. No production changes, dependencies, harness, network or existing financial/mail proof replays.

## Native producer boundaries

* SDK discovery advertises object inputs and tighter checks. Direct HTTP `from_slice` derives accept objects **and full positional arrays**, including Party/Line/SimplifiedTotals. Object optional fields may be omitted/null; sequence Option slots must still be present. Schemas preserve Rust declaration order, signed i64 range, numeric (not string) amounts, true boolean confirmations, unknown/duplicate object field rejection. Semantic guards run after deserialization, not all parse-valid requests succeed.
* Preview returns raw `application/pdf`, not `{ok,invoice:[bytes]}` and not base64. Issue returns `{ok:true,invoice:...}` with optional service usage/business stages, immutable original JSON/PDF evidence, and no implicit posting. Source-derived null fields in issued result are deliberate (e.g. DKK FX fields; domestic reverse-charge VAT aggregate). Numeric amounts are producer JSON numbers, not formatted strings.
* GET-backed HEAD retains method HEAD in the handler, whose `mutation = method != GET` classifies it as a mutation. All three selected HEAD routes require Origin and application/json InvoiceInput and fall through to **Issue** rather than Contacts/List/Pdf. Empty HEAD normally refuses; valid-body HEAD can allocate/publish/register an invoice. There are **no wire entity bytes**, even for middleware errors. Producer schemas describe constructor data only. Constructor content-type/cache headers persist; no HEAD PDF payload exists; successful Issue JSON constructor is stripped. HEAD is not a safe read probe.
* Human failure mapping uses native error message on 422; service mapping uses code plus usage/business stages. Neither is a fabricated generic HTTP failure. Middleware negotiation 409 is separate. All three negotiation headers absent remains legacy-major-1 compatibility, not an auth bypass.

## Query/order and authority

`year` only on the path ending /invoices; HEAD on that path accepts it but dispatches Issue and does not use it. Shared parser accepts `source=live|archive` but sales ignores it; this is **not archive readback**. `account`/`format` reject. Unknown or duplicate keys reject; empty query accepted, values bounded 128 UTF-8 bytes and not trimmed. No limit, cursor or client sort. Invoices are fiscal-date filtered then descending date/id; contacts are active name/id sorted with all-invoice balances and GDPR overlays. Original invoice readback routes are outside this partition's coverage, cited only for exact recovery context.

Current auth/roles, active company selection, admitted owner, immediate transaction, workspace-transfer and backup write fences, compiled posting rules, dates and closed periods remain. Preview never reserves a number. Publication precedes registration and can leave immutable artifacts after failure: published files are not alone proof of a committed issued invoice.

## No replay key promise

These routes do not consume native idempotency keys or return keyed receipts. POST Idempotency-Key is ignored by this adapter; service GET rejects it. There is no scope/TTL/tombstone to promise. Reusing an invoice post or linked imported bank receipt refuses, not replay-success. Partial credits can validly occur more than once: never automatically replay an unknown credit. Native settlement uses one unambiguous imported bank selector and the whole absolute imported amount/date, same currency and original receivable, not the current debtors role. Refund consumes outgoing evidence against refundable credit; credit preserves original account allocation and legacy/canonical proof.

On unknown writes, reconcile exact invoice document ID/number, original JSON/PDF hashes, posting journal identity, original readback payments/credits/refunds and imported bank reconciliation. Check current access/source/year before interpreting absence. Missing filtered records cannot prove rollback. A reused receipt must not post/send/reissue as new. No original-key retry guarantee is borrowed from other lifecycle endpoints. Public e-invoice status in list is existing durable state, not permission to resend uncertain delivery.

## Evidence

Provenance binds current advertisement/parser/dispatch/producer/failure/recovery source bytes. JSON Schema 2020-12 definitions are owned locally; no dependencies on parallel worker partitions. Existing Python oracle checks coverage/source hashes/resolution/eight fields; semantic source review remains main's responsibility. No existing #692/#709/#711 or other financial proof was rerun. No commit by worker (runner captures changes).

Lead source-review correction: Correct currentHEADrequiredOrigin headerdeclaration; actualHEAD->Issue defect preserved asdiagnosedproductgap, targetedcode/testrepair required beforeexternalpublication.
