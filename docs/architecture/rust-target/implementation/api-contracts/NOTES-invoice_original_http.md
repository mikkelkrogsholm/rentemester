# #677 selected original invoice HTTP contracts

## Scope and evidence

Exactly 17 identities: nine original lifecycle POST operations and GET/HEAD for
list, find, status and overdue. These are source documentation, not fresh
financial execution evidence. The existing invoice/mail/financial proofs,
including #692/#709/#711, were not replayed. Full #677 every-advertised-operation,
all-55, J01–J07 and final installed-desktop gates remain retained; the parent owns
acceptance. This partition neither closes the issue nor depends on parallel
workers' schemas. All references resolve within the owned schema file.

The provenance manifest and each operation bind current bytes with SHA-256 and
literal source needles: advertisement, router, strict parser, storage validation,
authorization/transaction fence, actual producer, failure mapping and recovery
sources. Client discovery schemas are not response proof. In particular the SDK
accepts a broad `data` object; the schemas here enumerate native output fields.

## Important wire distinctions

* Render is a confirmed POST returning a JSON receipt for **existing PDF
  evidence**, not PDF bytes and not an issue/post/send operation.
* Axum's implicit HEAD route reaches the GET-only handler with HEAD unchanged.
  Credential and selected-company failures may occur first; otherwise this
  handler constructs 400 `invalid_query`, never a successful invoice read.
  **Every HEAD wire body is empty**, including protocol middleware refusals.
  JSON constructor shapes for GET are not HEAD payloads.
* Original lifecycle parses `Unique` recursively: duplicate JSON object fields
  are rejected even when equal. It then applies an explicit object guard to the
  top-level and most source payloads, rejects explicit null recursively, and
  limits all arrays to 200 and text to 16,384 UTF-8 bytes without controls.
* Create payload is different: `serde_json::from_value::<Input>` accepts derived
  structs as objects **or exact positional arrays**. NativeInput, NativeParty,
  NativeLine and NativeSimplifiedTotals document declaration-order tuples.
  Missing Option object fields become None, but tuple fields without
  `serde(default)` must be present; explicit null still fails the earlier
  recursive bound. Required Input fields are issueDate and lines; Line requires
  description; SimplifiedTotals requires grossAmount and vatRate. SDK object-only
  advertisement is intentionally narrower. Structural acceptance is not a promise
  that every positional payload succeeds later prepare/computation/indexing.
* Native issue accepts line tax decisions, unitCode and buyer/order references
  beyond its SDK advertisement. Supplied source totals are verified against the
  engine, not trusted as computed values. Monetary output is JSON numeric
  `wire(Money)`, with finite/exact round-trip checks, not decimal strings. SQL
  histories preserve numeric/null/text storage types through the bounded rows
  producer. No blanket string/amount normalization is implied.
* The create computed wrapper always emits its six named fields. Missing native
  uniform vatRate or domestic reverse-charge vatAmount is indexed as JSON null;
  it is not omitted. Reduced descriptions and full computed tax lines are
  distinct shapes; DKK does not acquire foreign FX allocation fields.
* Native source refusals caught by lifecycle `apply`, and unresolved status,
  can be HTTP 200 `{ok:false,errors:[...]}` without code/data. Uncaught human
  not-committed errors are 422 with actual engine code/message; invalid session
  and denied current access retain 401/403. Service failures add snake_case
  usage/business stages and use their precise stage/reusability status mapping.
  Successful original-family service output does not add usage/business fields.

## Query, source and recovery controls

Queries are decoded form pairs: malformed percent escapes, decoded duplicate
keys, unknown keys, empty/control values, over-1,024-byte values and over-4,096-byte
raw queries fail. Amount filters parse finite JSON numeric lexemes, not arbitrary
numeric strings. Date validation is calendar-exact. Selector id wins over number.
The operation rows describe exact inclusive filters, normalization that actually
exists in the producer, deterministic ordering, and absence of pagination.
`asOf` changes aging only; current validated histories remain current. Each SQL
history is complete or fails at 5,000 rows / 8 MiB, rather than silently truncating.

No original endpoint supports a replay key: the header is rejected, body unknown
fields are rejected, and no TTL, key scope or tombstone is promised. All SDK calls
are single attempts. Post has duplicate-journal refusal and settlement has
bank-link refusal; those are not request replay receipts. Issue/create/credit and
manual payments do not become idempotent merely because their inputs match.

After unknown write outcome, first obtain current authoritative invoice readback
and verify immutable original number/document/hash/PDF bindings, then the exact
active journal, original receivable, bank evidence, amount/currency/date and
payment/credit/refund/repair links. Status alone is not proof of a particular
posting journal. A missing row is not proof of noncommit if company selection,
access or source integrity changed. Keep the outcome unknown when evidence is
unavailable. A reused receipt must never post, send or reissue as new. No recovery
step bypasses current authentication, permissions, backup/business-write fences,
closed-period controls, source integrity or revision controls elsewhere.

## Validation

Only the supplied strict Python source/schema oracle is used for this partition;
no Cargo build, server/browser/network invocation, new harness, dependencies or
financial effect replay is needed. Graphify query was performed before source
inspection; the required code-only extraction is isolated in the worker build
output. Oracle results are reported in the worker handoff, not fabricated here.
