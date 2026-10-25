# Original agent first use: company creation and workbench

Issue #717 component accepted at `305cfbf9f36eb85873430c13fb856bc8325b8dac`.
One common Rust server remains authoritative. Original `company_add` now exposes
its flat schema, requires confirmation before workspace resolution or HTTP, and
returns the original envelope. An absolute workspace must match the actual root
of a freshly verified explicit local profile; a logical workspace ID cannot stand
in for a path. Original `bookkeeping_workbench` forwards every flat filter, keeps
population blockers separate from page selection, and preserves canonical results.
Absolute company selection requires exactly one directory under that local root.

Combined acceptance executed 15 SDK and 10 MCP tests. Current refusal, unknown
outcome/no resend, confirmation/forged-field negatives, company isolation,
concurrent filters, local binding and schema/text/structured-content assertions
passed. The actual compiled catalogue contains 434 tools. All 1,618 typed
operation contracts passed the existing strict source/schema oracle; all prior
1,616 business contracts and definitions remain unchanged apart from source pins.

A test fixture initially expected a business request before mandatory protocol
discovery. Captured wire proved the cause; the existing credential-free discovery
fixture repaired it. Failed receipts and the isolated before/after proof remain.
No protocol or accounting control was bypassed, and accepted setup/financial
effects were not replayed.

Full #717 remains open: 13 original names and 29 workflow descriptions are absent;
classified ABI/annotation differences and full per-operation acceptance remain.
The original environment-variable workspace fallback is unavailable: omitted or
blank workspace uses the explicitly configured runtime. Absolute workspace
assertions require a local profile. No new live-use/cutover or installed desktop
acceptance is claimed.

Evidence: `/tmp/rentemester-rust-reports/r116-original-first-use717-component-accepted.json`.
