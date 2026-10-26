# External accounting evidence

`external_accounting_evidence` preserves a source document made by an external
accounting system without pretending it is a purchase invoice, cash receipt or
internal voucher. The first supported category is `payroll`.

The immutable metadata records the report's issue date, external issuer,
reported company, `YYYY-MM` accounting period, external reference and equal
positive debit/credit totals. Its VAT amount is always exactly zero. The source
file's SHA-256 and metadata are the evidence; Rentemester does not calculate
payroll, infer tax, create payments or submit a payroll filing.

After ingest, an authorised reviewer uses an ordinary journal dry-run to review
the exact accrual lines, posts that reviewed journal with a document link and
idempotency key, then handles a later bank settlement as a separate reviewed
journal. This retains the distinction between a payroll report, the legal
liability it documents and a bank movement.

For a machine-readable, live-operation workflow, start at `system_server_about`
(the full profile retains `meta_about` as a compatibility name) and search for
`external payroll evidence` in the agent capability catalogue.
