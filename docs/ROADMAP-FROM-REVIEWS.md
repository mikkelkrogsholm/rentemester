# Roadmap items from the agent + business-owner reviews

This document captures the work surfaced by the two fresh-eyes reviews
(`virksomhedsejer-review` + `integration-review`, two rounds each) that
was **explicitly deferred** from the rolling fix batches because it
requires more than a single session of work — new product surfaces,
schema-level changes, multi-week integrations, or political decisions
about scope.

Each item lists: what was surfaced, the current workaround, the
estimated scope, and the next concrete action if/when the item is
prioritised.

The fix batches that DID land are catalogued in the git history. The
batches up to commit `fb15d68` cover localisation (rounds 1–3), MCP
agent-DX, MCP gap-filling tools (audit_log_list, bank_account_list,
company_profile_get), and the round-2 polish batches D/E/F. See
`git log --grep="Batch"` for the trail.

---

## Product gaps (would block real-world adoption)

### Payroll / A-skat / AM-bidrag / ATP / eIndkomst

**Surfaced by:** virksomhedsejer-review (both rounds, top priority).
**Current state:** the chart-of-accounts has the right slots (3500
Lønninger, 7100 Skyldig A-skat, 7110 Skyldigt AM-bidrag, 7120 Skyldig
ATP) but nothing in `src/core/` registers a payroll period, computes
A-skat/AM-bidrag from a trækprocent, or exports to eIndkomst.
**Workaround:** keep a parallel payroll system (Danløn, Letløn, …);
manually book the monthly journal entries through `journal post`.
**Estimated scope:** new `src/core/payroll/` module, ~6-10 weeks
including DK-payroll-domain testing and eIndkomst CSV format compliance.
**Next action:** decide whether Rentemester targets sole-trader-only
(0 employees) or includes payroll. If included, scope a `payroll
register-monthly` CLI command + cockpit view + eIndkomst export as a
single sliced epic.

### PEPPOL / NemHandel — receive (not just send)

**Surfaced by:** virksomhedsejer-review round 1.
**Current state:** `invoice send-public-peppol` (write) + the
`peppol_submit_public_invoice` MCP tool can SEND OIOUBL BIS 3 invoices.
There is no inbox-style "receive an OIOUBL invoice from a vendor's
NemHandel mailbox" path.
**Workaround:** forward the email-attached OIOUBL XML to the bilagsmail
inbox and let the agent ingest it as a regular bilag.
**Estimated scope:** ~2-3 weeks; reuse the existing
`buildPublicEInvoiceOioUblXml` infrastructure for parsing, add a
NemHandel-mailbox poller analogous to `imap_intake_poll`.
**Next action:** spec the NemHandel-mailbox auth flow (CVR-bound
identity, the SMP lookup) and the parsing-side of the OIOUBL profile.

### PSD2 / open banking — direct bank feeds

**Surfaced by:** virksomhedsejer-review (both rounds).
**Current state:** CSV import only — `bank_import` (with `--profile
danske-bank` for that one bank's CSV; the generic parser auto-detects
the rest).
**Workaround:** download CSV from netbank weekly, run `bank_import`.
**Estimated scope:** ~4-6 weeks; depends on choice of PSD2-aggregator
(Tink/Nordigen/Saltedge/etc.) and DK-bank coverage. Adds a third-party
runtime dependency the project has so far avoided.
**Next action:** decide whether to take the third-party dependency.
Alternative: ship a long-tail of bank-specific CSV profiles as a
shorter-term mitigation.

### Real OECD SAF-T export

**Surfaced by:** virksomhedsejer-review round 2.
**Current state:** `system export-saft` produces XML in the
project-private namespace `urn:rentemester:dk:saft:v1` with the
`ProfileID = rentemester-dk-saft-v3-ledger-sales-purchases-masterfiles`
— useful as a deterministic export but NOT importable in any standard
SAF-T 1.0/2.0 tool a Danish revisor would use.
**Workaround:** the JSON-handoff (`system export-accountant`) is the
recommended path for revisor-handover today.
**Estimated scope:** ~3-4 weeks to produce real SAF-T XML against the
OECD schema. Naming question: rename the current `export-saft` command
to something honest (`export-rentemester-audit-xml`) in the same change,
or keep it as a stable name and add `export-saft-oecd` alongside.
**Next action:** pick a schema target version (OECD SAF-T 2.0 vs the
Danish-specific variants) and a naming strategy.

### Danish-language rules bundle (rules/dk/*.yaml)

**Surfaced by:** virksomhedsejer-review round 2.
**Current state:** `rules/dk/vat.yaml`, `bookkeeping.yaml`, etc. have
`name` + `explanation` fields in **English** ("Danish VAT deduction
requires stated VAT amount" — for a Danish-rules-first product).
**Workaround:** none — the English text surfaces in the cockpit's
`/lovgrundlag` view 1:1.
**Estimated scope:** ~1 week + translation review; add `name_da` +
`explanation_da` to the rule-bundle schema, populate for the existing
~120 rules, surface in the cockpit view.
**Next action:** extend the rule-bundle schema with the two `_da`
fields, then a single mechanical translation pass. Keep the English
fields for agent/MCP consumers.

### Cockpit AuditLogView (depends on `audit_log_list`)

**Surfaced by:** virksomhedsejer-review round 2 ("jeg vil have en
kronologisk audit-log-visning").
**Current state:** the MCP `audit_log_list` tool exists (Batch C); the
cockpit React side does not consume it yet.
**Workaround:** `rentemester gdpr audit-log --company …` from the CLI.
**Estimated scope:** ~2-3 days for a polished cockpit view with date
filter, type filter, actor filter and pagination over the existing MCP
tool / new HTTP route.
**Next action:** add a `GET /api/companies/:slug/audit-log` route
mirroring the MCP tool's filters, plus the React view + nav-tab entry.

### Cockpit write surfaces for the remaining CLI-only commands

**Surfaced by:** virksomhedsejer-review (both rounds), integration-review
(parity table).
**Current state:** these write commands have no cockpit equivalent:
period reopen, opening-balance post, bank-account add,
company set-profile (partial), GDPR forget/discover (cockpit has only
the export), recurring-invoice retire, asset write-off (cockpit has it
but not the bilag-search modal), system backup-add-destination,
system rotate-backup-keypair, "Lav backup nu" one-click button.
**Workaround:** CLI.
**Estimated scope:** ~1 day per cockpit view; ~2 weeks for the full set
including the GDPR forget flow (which needs the most care).
**Next action:** prioritise by frequency. The "Lav backup nu" button on
IntegrityView is the single highest-leverage gap (it's blocking trust
in the backup story).

---

## Agent-surface scaffolding (deferred from Batch B/F)

### Pagination on the remaining list tools

**Surfaced by:** integration-review (both rounds).
**Current state:** the `paginationFields` contract from
`src/mcp/pagination.ts` is applied to `bank_list`, `journal_list`,
`documents_list`, `customer_list`, `vendor_list`, `audit_log_list`.
**Still missing on:** `invoice_list`, `invoice_find`, `invoice_overdue`,
`accounts_list`, `period_list`, `mileage_list`, `payable_list`,
`accrual_register_report`, `asset_register_report`, `budget_list`,
`recurring_invoice_list`, `exceptions_list`, `import_archive_list`.
**Workaround:** small companies don't hit the cardinality; large ones
get untruncated responses that can exceed an agent's context window.
**Estimated scope:** ~30 minutes per tool × 13 tools, plus per-tool
fixture in the pagination test (`mcp-list-pagination.test.ts`).
Mechanical but tedious.
**Next action:** schedule a single sweep batch.

### Per-tool `dataSchema` on `outputSchema`

**Surfaced by:** integration-review round 2.
**Current state:** every MCP tool's `outputSchema` is the shared
`envelopeShape` whose `data` field is `z.object({}).passthrough()`. The
real per-tool result shape (`InvoiceListResult`, `JournalPostResult`,
…) is documented in `docs/mcp-tool-surface.md` prose, NOT in the
schema. An agent doing schema-introspection sees nothing about the
result.
**Workaround:** the agent reads the description prose.
**Estimated scope:** architectural — each tool would need to declare a
per-tool zod schema and wrap it inside the envelope. ~3-4 weeks
including TS type plumbing to keep the wrap+unwrap type-safe.
**Next action:** pick 5 high-impact write tools (`journal_post`,
`invoice_issue`, `expense_book`, `bank_import`, `system_backup`) and
ship per-tool data schemas as a first slice; leave the rest for a
follow-up if the pattern proves useful.

### Real idempotency-key dedup cache

**Surfaced by:** integration-review (both rounds).
**Current state:** Batch F-3 added the `idempotencyKey: z.string()`
schema field to the shared `tool-runtime`, but the key is currently
RESERVED — the actual server-side dedup cache isn't built yet, so a
duplicate call with the same key still double-books. The agent
contract docs the trap explicitly.
**Workaround:** the agent must read state back (e.g. `invoice_list`)
before retrying a failed `*_post` write.
**Estimated scope:** ~1-2 weeks — new `mcp_idempotency_keys` table
keyed by `(company, tool, idempotencyKey)` with a TTL, surfaced via a
shared wrapper in `tool-runtime.ts`. Must integrate cleanly with the
existing audit-log so a deduplicated call STILL appears in the
revisionsspor as "served from cache".
**Next action:** schema migration + wrapper helper + tests for the
five highest-blast-radius writes first.

### `rentemester_class` custom annotation per tool

**Surfaced by:** integration-review round 1.
**Current state:** the four safety classes (read / write_reversible /
write_irreversible / destructive) are described in
`docs/mcp-tool-surface.md` but only `readOnlyHint` and `destructiveHint`
are first-class fields on tool annotations. An agent that wants to
mechanically filter "show me every write_irreversible tool" has to
parse description prose.
**Workaround:** the prose is consistent and grep-able.
**Estimated scope:** ~1 day to add a custom annotation field on every
tool (~98 tools), plus a docs test that asserts every tool has the
class set and that it agrees with the existing prose classification.
**Next action:** mechanical sweep.

### Async / job-id model for long-running ops

**Surfaced by:** integration-review (both rounds).
**Current state:** `system_backup` (with archive:true), `system_export_*`
(authority/SAF-T/accountant), `bank_import` (50k+ rows),
`imap_intake_poll` (network-bound) all run synchronously and can exceed
an agent's tool-timeout. There's no `system_backup_status_check(jobId)`
polling pattern.
**Workaround:** call from a process with a long timeout (the CLI).
**Estimated scope:** ~3-4 weeks — needs a job-store table, a
background-runner, and per-tool refactor to enqueue + return a jobId
instead of blocking.
**Next action:** scope a single op first (`system_backup --archive`),
get the pattern right, then mechanically convert the others.

---

## CLI / cockpit UX polish (deferrable but not multi-week)

The remaining UX polish items not yet landed sit in the cockpit and
the CLI:

- `Saldobalance` PDF/CLI doesn't warn when no opening balance is posted
  (the negative-bank-as-asset trap from virksomhedsejer-review round 2).
- Statement PDFs miss `kr.` / `DKK` suffix on number columns.
- Dashboard "Sådan kommer du i gang" actions aren't numbered.
- `gdpr forget` only has CLI surface (cockpit can only `export`).
- `customer create` doesn't validate email format.
- The 21-tab cockpit nav is overwhelming; grouping should collapse.
- CompanyForm at create doesn't capture address/postal/city/IBAN.
- Cockpit serves Google Fonts via CDN (GDPR concern).
- `confirm()`-replacement-with-ConfirmDialog landed for Suggestions /
  Assets / Gdpr / Bilagsmail; verify no other view kept a raw
  `window.confirm` / `window.prompt`.

Each is ≤1 day of work and a good candidate for a per-tab Batch H sweep
when the larger items above start landing.

---

## Tracking

When an item from this list ships, MOVE its section to the historical
section at the bottom of the file with the landing commit hash so the
remaining roadmap stays scannable.

(No items landed from this list at the time of this commit.)
