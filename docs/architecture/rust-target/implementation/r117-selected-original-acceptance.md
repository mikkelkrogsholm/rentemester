# Correct selected original in shared document intake

Issue #718 component accepted at `9454e4ad39275d10fd22c797c8ef588241404555`.
The existing shared React form immediately invalidates obsolete original bytes
when its file selection changes. It shows reading status and blocks explicit
submission until the current read finishes. Superseded success/failure, clearing
and dismissal cannot adopt old bytes. An unreadable current original exposes the
existing actionable error; a new valid choice recovers. Existing metadata,
current server authority, scanner, hashing and unknown-outcome controls remain.

Four new product-race cases failed on the old code; five current race/error
cases and all 19 modal cases pass after correction. The shared UI design check,
TypeScript check and build pass through SGD. A fresh empty-task integration was
missing existing dependencies; reuse of the current dependency links fixed that
environment fault without an install, check change or production-code change.

The actual retained normal-MFA browser exercised pending replacement, late old
success, current read failure and fresh-file recovery against the qualified common
Rust server. Its helper then requested a purchase-only description field while
a cash-register receipt was selected. It stopped before any ingest, scan or
financial effect. The failed receipt is preserved. Two setup faults reached the
agreed boundary: no further full runtime/browser setup was started. Existing
server original/scanner evidence is retained, not relabelled as a new R117 upload.

A fresh complete upload and canonical byte/hash readback are **not qualified by
R117**. Full original #718 scope and all55/release/cutover/final installed desktop
requirements remain open.

Evidence: `/tmp/rentemester-rust-reports/r117-selected-original718-component-accepted.json`.
