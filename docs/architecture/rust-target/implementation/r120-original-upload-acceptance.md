# Original upload and documented cash-receipt supplier

Issue #718 component accepted; shared UI source `47b012d3c1ddc11fd11605cbc539947a38be14a1`.
Current common Rust server source `996df249fe37d5ba675dd12e2d155cfb3b2b1997`
was guarded-built and exercised with the retained ordinary MFA owner. Exactly one
selected original passed the required scanner, canonical byte/hash equality and
reopened list, while journal/trial balance/VAT stayed unchanged. A read-only resume
after actual server restart independently confirmed the same original and sole
previous journal, zero new mutations and current MFA-backed session.

The first report preserves a helper failure after successful upload: the booking
page has both h1 and h2 headings. The read-only resume used its exact h1 and never
replayed upload. It exposed a real supplier-evidence refusal: cash-receipt intake
hid the supplier fields required for later expense posting. The shared React form
now exposes the existing explicit supplier controls for receipts while keeping
invoice-only recipient/description fields absent. Two regression cases fail on
old code; all20 modal cases and existing design/type/build pass. Empty supplier
upload remains allowed for intake; server posting controls remain unchanged.

A subsequent complete-bookkeeping helper refused before any upload/bank/financial
mutation because it assumed one bank account. Read-only retained SQL and the
actual producer show separate default and explicit mapped accounts; the helper
selector is now isolated against that evidence. Two setup faults reached the
agreed boundary: no further full runtime setup was started.

New supplier controls have not completed actual bank→expense→backup acceptance.
The original unresolved receipt is preserved. Full #718, all55/J01-J07/cutover
and final installed desktop acceptance remain open.

Evidence `/tmp/rentemester-rust-reports/r120-original-upload-and-cash-supplier718-component-accepted.json` (SHA-256 `b473f66f7fd6ae60842d0185162691015a8b405fea4a97ccd640b9c7c5de3220`).
