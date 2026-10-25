# Original company diagnostics

Issue #717 component accepted at `43d904d6a16a5c2813d253f8c8e1961ac3cbec0f`.

The common Rust server now provides the original dated retention read and company
health diagnostics to SDK/MCP. Invalid dates retain their original business error.
Current human/service authority remains fenced throughout the read; a damaged
registered secondary company returns meaningful failed checks without opening its
ledger. Original flat input, error/data envelopes and native interfaces are retained.

Actual integrated checks pass: 3 application, 2 host, 2 HTTP, 6 SDK and 5 MCP cases,
plus generated protocol verification. The actual compiled catalogue has 436 tools.
All 1,624 typed operations match current HTTP/MCP surfaces; the previous 1,618
business contracts remain unchanged except source hashes.

The inspector exposed a real remaining gap: fresh native companies provision the
minimum SQL namespace without the original config/documents/inbox layout. The
unit fixture now tests both incomplete and complete layouts; production is not
falsely marked healthy. Protected compatible provisioning remains necessary.

Preserved failures distinguish that fixture assumption, a test-only Rust borrow
error, and an isolated adapter's older protocol fixture. No additional live runtime
or browser setup was started. A new normal-MFA diagnostic journey and qualified
server rebuild are not claimed. Full #717, all55, release/cutover and final installed
desktop acceptance remain open.

Evidence: `/tmp/rentemester-rust-reports/r118-original-diagnostics717-component-accepted.json` (SHA-256 `b5e2dca7991a0bc96337bc065d4a94716833f10fe2f0152fc78985e692fdb3c3`).
