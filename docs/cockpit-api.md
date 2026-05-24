# Cockpit HTTP API — the `rentemester serve` interface contract (#296)

`rentemester serve` starts a local JSON HTTP API over a workspace and its
`src/core/` bookkeeping engine. It is the backend consumed by the (separate)
React cockpit app, but it is a plain JSON API and can be driven by any HTTP
client.

This document is the interface contract: the endpoints, their request
payloads, their response shapes and the envelope/error contract. The request
logic lives in `src/server/` — `router.ts` (dispatch + read routes),
`write-handlers.ts` (the bookkeeping write routes) and `mutations.ts` (the
shared write pipeline).

It is a sibling of [`docs/mcp-agent-contract.md`](mcp-agent-contract.md) (the
MCP loose-tool surface) and [`docs/runtime-agent-contract.md`](runtime-agent-contract.md)
(the packaged `agent run` loop). All three are callers of the SAME core; the
Cockpit is the third write-stack.

## Starting the server

```
rentemester serve \
  --workspace <dir> \
  --host 127.0.0.1 \
  --port 4319
```

| Flag | Meaning |
|------|---------|
| `--workspace` | The workspace directory the API serves. Falls back to `RENTEMESTER_WORKSPACE`. Required (one of the two). |
| `--host` | Interface to bind. Default `127.0.0.1` (localhost-only). Also `RENTEMESTER_APP_HOST`. |
| `--port` | TCP port. Default `4319`. Also `RENTEMESTER_APP_PORT`. |

Environment:

| Var | Effect |
|-----|--------|
| `RENTEMESTER_WORKSPACE` | Workspace root, when `--workspace` is omitted. |
| `RENTEMESTER_APP_HOST` / `RENTEMESTER_APP_PORT` | Bind host/port, when the flags are omitted. |
| `RENTEMESTER_APP_AUTH` | When set to `required`, every request needs a bearer token. Default (unset) is **localhost-trusted** (Phase 1). |
| `RENTEMESTER_APP_TOKEN` | The shared-secret bearer token consulted only when `RENTEMESTER_APP_AUTH=required`. |
| `RENTEMESTER_APP_STATIC` | Override the built cockpit SPA directory (`app/dist`). |

The API is **workspace-scoped**: it serves an entire workspace, and every
company-scoped route addresses a company by its workspace **slug** (not a
filesystem path). This differs from the MCP surface, which also accepts a
path.

## Authentication

There is one auth seam (`src/server/auth.ts`), run before every route.

- **Phase 1 (default, `RENTEMESTER_APP_AUTH` unset):** localhost-trusted. No
  credentials. The server is bound to `127.0.0.1` and trusts the caller
  because it is local.
- **`RENTEMESTER_APP_AUTH=required`:** a shared-secret bearer token. Every
  request must carry `Authorization: Bearer <RENTEMESTER_APP_TOKEN>`; a
  missing or wrong token is `401`.

**The localhost write hard-gate.** When auth is *disabled* (Phase 1), a
bookkeeping **write** is additionally refused unless the request's `Host`
header is a loopback address (`127.0.0.1`, `localhost`, `::1`). A non-loopback
host with no auth would be an unauthenticated write from the network, so it
fails closed with `401`. Read routes are not subject to this gate.

## The response envelope

Every response is JSON with `content-type: application/json; charset=utf-8`.

**Success** — HTTP `200` (or `201` for company creation) — always has
`ok: true` plus exactly one named payload key:

```json
{ "ok": true, "<key>": { ... } }
```

The `<key>` is route-specific (`dashboard`, `invoices`, `import`, `invoice`,
…) — see each endpoint below.

**Error** — any non-2xx — always has this shape (`src/server/errors.ts`):

```json
{ "ok": false, "error": { "code": "<code>", "message": "<safe message>" } }
```

| `error.code` | HTTP | When |
|--------------|------|------|
| `bad_request` | 400 | Malformed body, bad/missing field, a core business rejection that is not a conflict. |
| `unauthorized` | 401 | Missing/invalid bearer token, or a write from a non-loopback host with auth disabled. |
| `not_found` | 404 | Unknown company slug, company with no ledger, or an unknown endpoint. |
| `method_not_allowed` | 405 | Right path, wrong HTTP method. |
| `conflict` | 409 | The backup lock is active, or a state conflict — the target is missing, or the action already happened (e.g. an already-posted invoice, an already-resolved exception). |
| `internal` | 500 | An unexpected error. The real message is **never** leaked — the body always reads `internal server error`. |

A core business rejection (`ok:false` from a bookkeeping function) is mapped
to `bad_request` by default, or to `conflict` when the message indicates a
missing target or an already-done action. It is **never** a `500`.

## Read endpoints

All read endpoints are `GET`, side-effect free, and require no body. Unknown
slug → `404`. The `year` query parameter, where accepted, selects a fiscal
year; `asOf` (a `YYYY-MM-DD`) selects an as-of date — both default sensibly
when omitted.

| Method + path | Response key | Purpose |
|---|---|---|
| `GET /api` or `GET /api/health` | `service`, `workspace`, `authRequired`, `routes` | Health probe + server identity + route-catalog (#376). `routes` is a machine-readable list of every HTTP endpoint with `{ method, pattern, summary }` so an agent can enumerate the surface without reading source. The catalog is the same `ROUTE_CATALOG` exported from `src/server/router.ts`. |
| `GET /api/portfolio?asOf=` | `portfolio` | Cross-company portfolio overview. |
| `GET /api/companies` | `workspace`, `count`, `companies[]` | List workspace companies (`{slug,name,createdAt,archived}`). Discovers and adopts an unlisted-but-present company directory before listing. |
| `GET /api/companies/:slug/dashboard?asOf=` | `dashboard` | The company dashboard data. |
| `GET /api/companies/:slug/fiscal-years` | `fiscalYears` | The company's fiscal years. |
| `GET /api/companies/:slug/overview?year=` | `overview` | Per-year overview. |
| `GET /api/companies/:slug/income-statement?year=` | `incomeStatement` | Income statement (resultatopgørelse). |
| `GET /api/companies/:slug/balance?year=` | `balance` | Balance sheet (balance). |
| `GET /api/companies/:slug/trial-balance?year=` | `trialBalance` | Trial balance (saldobalance). |
| `GET /api/companies/:slug/journal?year=&account=` | `journal` | Journal entries, optionally filtered by account. |
| `GET /api/companies/:slug/bank?year=` | `bank` | Bank transactions. |
| `GET /api/companies/:slug/vat?year=` | `vat` | VAT report (momsopgørelse). |
| `GET /api/companies/:slug/documents` | `documents` | Ingested documents (bilag). |
| `GET /api/companies/:slug/archive/:year` | `archive` | The archived bookkeeping for one year. |
| `GET /api/companies/:slug/multi-year` | `multiYear` | Multi-year comparison. |
| `GET /api/companies/:slug/invoices?year=` | `invoices` | Issued invoices. |
| `GET /api/companies/:slug/contacts` | `contacts` | Customers + vendors. |
| `GET /api/companies/:slug/company` | `company` | Company settings / master data. |
| `GET /api/companies/:slug/obligations?year=` | `obligations` | Statutory obligations + deadlines. |
| `GET /api/companies/:slug/cashflow?year=` | `cashflow` | Cash-flow view. |

The detailed object shape of each read payload is the corresponding
`build*` function's return type in `src/server/data.ts`.

## Workspace-management endpoints

| Method + path | Body | Response key | Purpose |
|---|---|---|---|
| `POST /api/companies` | `{ name, slug?, cvr?, fiscalYearStartMonth?, fiscalYearLabelStrategy? }` | `company` (`{slug,name}`) | Create a new company. `201` on success; `409` if the slug already exists. |
| `PATCH /api/companies/:slug` | `{ name?, archived? }` (at least one) | `company` (`{slug,name,createdAt,archived}`) | Update a company's display name and/or archived flag. Never touches the slug or the ledger. |
| `POST /api/companies/:slug/sync-cvr` | _none_ | `sync` | Refresh CVR-register master data server-side (so CVR credentials never reach the browser). A failed lookup is reported inside `sync.ok`, not as an HTTP error. |

## Bookkeeping write endpoints

All write endpoints are `POST`, take a JSON body, and run through the shared
`withCompanyMutation` pipeline (`src/server/mutations.ts`): localhost
hard-gate → company resolution → confirm gate → open+migrate db → **backup-lock
gate** → actor attribution → core call → business-result mapping. The Cockpit
never reimplements bookkeeping — each handler is a third caller of the same
`src/core/` function the CLI and MCP use.

**The confirm gate.** A write that appends an irreversible ledger entry
requires `"confirm": true` in the body; without it the call is rejected with
`400` (`denne handling er irreversibel og kræver 'confirm: true'`) before the
core runs. The two non-irreversible writes (`exceptions/:id/resolve`,
`invoices/issue`) do **not** require `confirm` — the cockpit modal is the
human's consent.

**The backup-lock gate.** When the workspace owner has opted into the
BEK 205/2024 §4 backup lock and a weekly backup is overdue past the grace
window, every bookkeeping write is refused with `409` and the shared Danish
backup-lock message. Take a backup to unlock.

### `POST /api/companies/:slug/exceptions/:id/resolve`

Clears an open exception. `:id` must be a positive integer.

- Body: `{ note?: string }` (an empty body is allowed).
- No `confirm` required (non-destructive — the status only flips to
  `resolved`).
- Response key `exception`: `{ id, resolved }`.

### `POST /api/companies/:slug/bank/import`

Imports a bank-statement CSV. The frontend reads the chosen CSV file in the
browser and POSTs its text inline as `csvContent`.

- Body: `{ csvContent: string, account?: string, profile?: string, confirm: true }`.
- `confirm: true` **required** (it appends ledger rows).
- Body size cap: 12 MiB; a larger body is `400`.
- Response key `import`:
  `{ importBatchId, imported, skippedDuplicates, skippedDuplicateRows[], bankAccountSlug, profile, balanceWarnings[], exceptionsCreated }`.

### `POST /api/companies/:slug/documents/ingest`

Ingests a voucher/document (bilag). The document file is binary, so the
frontend base64-encodes it; the original filename extension is preserved (the
core resolves the MIME type from it).

- Body: `{ fileName: string, fileBase64: string, metadata: {...}, vendorId?: number, force?: boolean, confirm: true }`.
- `metadata` is the document-metadata object (mirrors the MCP `documents_ingest`
  input): `source` is required; `documentType` is `"purchase_sale"` or
  `"cash_register_receipt"`; optional `issueDate`, `invoiceNo`,
  `deliveryDescription`, `amountIncVat`, `currency`, `sender`, `recipient`
  (`{name?,address?,vatOrCvr?}`), `vatAmount`, `paymentDetails`, `exemptionCode`
  (`"FOREIGN_PHYSICAL_ONLY"` or `null`). Amounts are kroner.
- `confirm: true` **required** (it hash-stores the bilag and may post).
- Body size cap: 12 MiB; a larger body is `400`.
- Response key `document`: `{ id, documentNo }`.

### `POST /api/companies/:slug/invoices/issue`

Issues a sales invoice. Rentemester **computes** every line total, the net
amount, the VAT amount and the gross amount from the human's minimal input —
the same compute path as the CLI's guided `invoice create`.

- Body: `{ issueDate: string, lines: [{ description, quantity, unitPriceExVat }], vatRatePercent?: number, customerId?: number, buyer?: {name?,address?,vatOrCvr?}, seller?: {name?,address?,vatOrCvr?}, invoiceNumber?: string, dueDate?: string, currency?: string }`.
- `lines` is required and non-empty; `vatRatePercent` defaults to `25`;
  `currency` defaults to `DKK`. A `customerId` back-fills the buyer from
  master data.
- No `confirm` required — issuing produces a kladde (no journal entry yet);
  the multi-line modal is the consent.
- Response key `invoice`:
  `{ documentId, invoiceNumber, netAmount, vatRate, vatAmount, grossAmount, lines[] }`
  — the computed amounts are echoed back so the modal can show what
  Rentemester worked out.

### `POST /api/companies/:slug/invoices/post`

Posts an issued invoice to the ledger.

- Body: `{ invoiceDocumentId: number, transactionDate?: string, confirm: true }`.
- `confirm: true` **required** (write-irreversible — it appends a journal
  entry). A double-post is refused by core and mapped to `409`.
- Response key `posting`: `{ entryId, entryNo }`.

### `POST /api/companies/:slug/invoices/settle`

Settles an issued invoice against a bank payment.

- Body: `{ invoiceDocumentId: number, bankTransactionId?: number, bankTransactionReference?: string, paymentDate?: string, amount?: number, confirm: true }`.
  One of `bankTransactionId` / `bankTransactionReference` is required.
- `confirm: true` **required** (write-irreversible — it links a bank receipt
  and appends a journal entry). A double-settle is refused by core and mapped
  to `409`.
- Response key `settlement`:
  `{ entryId, paymentId, principalAmount, claimAmount, invoiceNumber, openBalance }`.

## Actor attribution

Every Cockpit write is attributed: `withCompanyMutation` resolves a
`Principal` from the auth seam into a core `ActorContext`, passed explicitly
into the core call (never via a process env var, so parallel requests cannot
collide). Phase 1 attributes to the fixed localhost web actor. The actor is
written into `audit_log.actor`, giving a traceable chain from the HTTP request
to the append-only posting.

## Static SPA

Any non-`/api` path serves the built cockpit SPA (`app/dist`, with an
`index.html` fallback) when it has been built. When no SPA is present, `/` is
a friendly health probe and any other non-`/api` path is a JSON `404`.

## Relationship to the other contracts

| | Cockpit HTTP API (this doc) | MCP loose tools | `agent run` loop |
|---|---|---|---|
| Driver | The cockpit SPA / any HTTP client | An external MCP client/agent | The in-process `runAgentLoop()` |
| Scope | A whole workspace; company by slug | One company per call; slug or path | One company, one run |
| Surface | A small REST-ish route set | 95 loose tools | A single fixed loop |
| Writes | 6 `POST` routes via `withCompanyMutation` | Write tools with `confirm` | The loop books deterministically |

All four rest on the same `src/core/`, the same rules and the same
append-only ledger.
