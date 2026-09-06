# MCP agent contract — the standalone tool surface (#203)

This is the operating contract for an **external agent** (Claude Desktop,
Cursor, Claude Code, Codex, …) that drives Rentemester through the MCP
server's compact gateway surface or its explicit backward-compatible full
profile. The default `compact` profile lists exactly eight tools; the internal
registry still contains every precise operation.

It is the sibling of [`docs/runtime-agent-contract.md`](runtime-agent-contract.md),
which covers the *packaged* `agent run` loop — a deterministic, replayable
in-process orchestrator with a fixed phase order. **This document is
different.** Here there is no loop, no fixed sequence, no `src/agent/`
orchestrator: there is a flat catalogue of tools and an agent that decides
which to call. This document is the contract that keeps that safe.

The server hands a compressed version of this contract to every client in
the `initialize` response's `instructions` field. This file is the full
form. The authoritative tool catalogue is
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md).

## Runtime outcome discovery (#584)

Start every new integration with `system_server_about`. Its `data.catalogue`
and registry digest carry schema/version identity; the entry point is
`system_server_about -> agent_capability_search -> agent_workflow_describe ->
agent_operation_search -> agent_operation_describe`. Call
`agent_capability_search` with a narrow outcome query (and follow its cursor)
before calling `agent_workflow_describe` for the selected `id`. The description
resolves its MCP operation references against the internal registry and
distinguishes `directlyListed` from operations available through
`agent_operation_read`, `agent_operation_write` or
`agent_operation_destroy`. It does not mutate a company. `tools/list` is fixed
at startup and never changes after search, describe or execution.
Each search result also includes the live CLI, MCP and HTTP operation bindings
for that capability, including canonical identity, schema hash, direct-vs-
gateway availability, safety, actor/confirmation and retry metadata.

### Profiles and canonical operation gateways

The stdio server selects `compact` by default. Set
`RENTEMESTER_MCP_PROFILE=full` before startup to expose all 246 legacy names
directly with their existing schemas and behavior. An unknown profile fails
startup. Full-profile names remain compatibility aliases; persisted keys,
permission names and audit identities are not renamed.

Every captured operation has one immutable lowercase-snake canonical identity
with explicit domain/resource/action metadata, an operation identity hash and
an exact input/output schema identity hash. Use `agent_operation_search` and
`agent_operation_describe` rather than deriving names from underscores.
The reviewed naming source is `src/mcp/operation-naming.ts`; legacy original
names stay unchanged for permissions, idempotency receipts and audit records.
`agent_operation_read`, `agent_operation_write` and
`agent_operation_destroy` invoke exactly one selected operation. They perform
the original schema validation, then re-run the selected original
authorization, membership, company-isolation, confirmation, backup/period,
retry/idempotency and audit gates. A gateway never plans, infers arguments or
batches calls.

`bookkeeping_batch_plan` remains read-only. The mutating canonical operation is
`bookkeeping_batch_persist`; `bookkeeping_batch_dry_run` is only a deprecated
compatibility alias/response adapter and is never described as read-only.

Example: to reconcile bank activity, call `system_server_about`, search
`reconcile bank`, describe `bank-reconciliation-batch`, then follow its read
and operation-gateway steps,
review the proposed batch, and only enter the apply boundary with the stated
actor and confirmation. Read back the canonical reconciliation and journal
records after an acknowledged write; do not retry an uncertain write blindly.

The equivalent read-only HTTP representation is `GET /api/agent-capabilities`
(`query`, `cursor`, `limit`) and `GET /api/agent-workflows/:id`. HTTP resolves
its canonical HTTP and CLI references; use MCP `tools/list`/describe when the
live MCP registration and annotations are required.

`system_server_about.data.catalogue.coverage` identifies the reviewed coverage rules and
the three public surface baselines. CI runs the same deterministic gate against
the live registries. A release candidate then runs that gate again inside the
published digest-pinned image and binds the resulting report checksum to the
release manifest. Adding, removing or misclassifying a public operation therefore
fails before release instead of silently disappearing from agent discovery.

## What the surface is

Rentemester exposes its bookkeeping core as 246 precise internal operations
over stdio (`src/mcp/server.ts`, registered by `src/mcp/registry.ts`). Compact
mode lists eight gateway tools; full mode lists all precise legacy names. Each
operation maps to a single core operation — issue an invoice, post a journal
entry, list bank transactions, take a backup, and so on.

There is **no conversation state on the server.** Every call is
self-contained: it carries its own `company` path and its own arguments.
The server does not remember the previous call. The agent is the only thing
holding the thread of work together.

## Identification — `company` is mandatory and explicit

- Every company-scoped tool takes an explicit `company` argument. There is
  never an implicit "current company". This is deliberate: it makes
  cross-company mistakes structurally impossible.
- The `company` argument accepts **either** of two forms (`resolveCompanyArg`
  in `src/mcp/tool-runtime.ts`):
  - an **absolute filesystem path** to the company directory — resolved and
    `..`-guarded, mirroring the CLI's `--company` guard; **or**
  - a **workspace slug** — a bare, separator-free, slug-shaped token, looked
    up in the manifest of the workspace named by the `RENTEMESTER_WORKSPACE`
    environment variable on the server's host. An unknown slug, or a slug
    given with no workspace configured, is an error.
  A value containing a `/` or `\` is always treated as a path, so a real
  path can never be misread as a slug.
- Workspace-level tools (`company_add`, `portfolio_overview`) take a
  `workspace` path instead.
- A wrong or missing path/slug comes back as `{ ok: false, errors: [...] }`.
  The error text never leaks the absolute host path back to the caller.

## Safety classes — read each tool's `annotations`

Every tool carries MCP `annotations`. Read them before calling. The four
classes (full table in `docs/mcp-tool-surface.md`):

| Class | How to recognise it | What the agent must do |
|---|---|---|
| `read` | `annotations.readOnlyHint: true` | Call freely, in parallel. No side effects. |
| `write-reversible` | write tool, state can be undone | Pass `confirm: true`. Reversible via a later correction/archive. |
| `write-irreversible` | write tool, posts into the append-only chain | Pass `confirm: true`. Can only be undone with a *counter-posting*. |
| `destructive` | `annotations.destructiveHint: true` (`system_restore_backup`) | Pass `confirm: true` **and** `confirmText: "RESTORE <targetCompany>"`. |

### The confirm convention

> **Cross-stack opslag.** Den fulde tabel pr. business-operation —
> "hvilke MCP-tools, cockpit-routes og CLI-kommandoer kræver confirm, og
> i hvilken syntax" — står i
> [`docs/confirm-contract.md`](confirm-contract.md). Samme operation kan
> have **modsat regel** på cockpit (`POST /invoices/issue` kræver det
> ikke; `invoice_issue` her gør) — afvigelsen er bevidst og forklaret
> dér.

Write tools refuse to run unless the arguments contain `confirm: true`.
Without it the server returns
`{ ok: false, errors: ["confirm: true required for write tool <name>"] }`
**before the core is ever called**. This is intentional friction: an agent
cannot post by accident, and a dry "what would happen" call is impossible —
deciding to write is always an explicit, logged act.

> **Exception — the destructive tool says `destructive`, not `write`.** The
> one `destructive` tool, `system_restore_backup`, returns
> `confirm: true required for destructive tool system_restore_backup` — the
> word **destructive**, not **write**. An agent that string-matches
> `required for write tool` will therefore MISS the restore tool.

**Branch on the envelope `code`, not on the message string.** Both refusal
variants carry the same stable machine-readable marker
`code: "CONFIRM_REQUIRED"` on the envelope, so an agent never needs to
parse `errors[]` to detect a missing confirm. The full set of stable
cross-cutting codes is `CONFIRM_REQUIRED` (missing `confirm: true`, write
*and* destructive), `CONFIRMTEXT_MISMATCH` (missing/wrong `confirmText` on
`system_restore_backup`) and `BACKUP_LOCKED` (bookkeeping lock — see "The
backup lock" below); the table lives in `docs/mcp-tool-surface.md` under
"Cross-cutting preconditions". String-matching the shared prefix
`confirm: true required for` (which catches both the
`confirm: true required for write tool ` and
`confirm: true required for destructive tool ` variants) is only a fallback
for clients that cannot read `code`.

`confirm: true` is not a rubber stamp. The agent should only set it once it
has gathered the preconditions (read first — see below) and is committing to
the mutation.

#### When the missing-`confirm` reply is *not* an envelope (`-32602`)

The `{ ok:false, errors:[...] }` envelope above only appears when the rest of
the payload is **schema-valid**. The server's `confirm` field is deliberately
optional, so a payload that is otherwise well-formed but simply omits
`confirm` reaches the handler and gets the envelope.

There is **one exception an agent must branch on.** If `confirm` is omitted
*and* the payload also has a **schema error** (a required field missing, an
empty `lines[]`, a wrong type, …), the MCP SDK's input validation runs and
rejects the call **before the handler — and therefore before the `confirm`
check — is ever reached.** In that case the reply is not the envelope but a
raw JSON-RPC error:

```json
{
  "result": {
    "content": [
      { "type": "text", "text": "MCP error -32602: Input validation error: Invalid arguments for tool journal_post: [ ... ]" }
    ],
    "isError": true
  }
}
```

This reply has **`isError: true`, no `structuredContent`, and no `errors[]`
array.** An agent that always reads `errors[]` from the envelope must guard
against it:

- **Detect it:** the JSON-RPC `result` has `isError: true` but no
  `structuredContent` (equivalently, the text content begins with
  `MCP error -32602: Input validation error`). There is **no top-level
  `error` field** on this reply — the `-32602` code appears only inside
  `content[0].text`.
- **Read it:** the human-readable cause is in `content[0].text` — it names
  the tool and lists the offending fields (zod issues with `path` and
  `message`).
- **Fix it:** treat it exactly like an `ok:false` precondition failure —
  correct the payload (supply the missing/typed field) **and** add
  `confirm: true`, then re-call. The `-32602` form never indicates a
  successful or partial write; nothing was posted.

In short: branch on `isError === true && structuredContent === undefined`
first; only then fall through to reading `structuredContent.errors[]`.

### The destructive convention

`system_restore_backup` is the only `destructive` tool. On top of
`confirm: true` it requires `confirmText` to equal the exact string
`RESTORE <targetCompany>`. A mismatch is rejected with
`confirmText must match 'RESTORE <targetCompany>'`. Restore can overwrite
files in the target company directory — the double confirmation is the
guardrail against pointing it at the wrong directory.

Both gates yield the normal `{ ok:false, errors:[...] }` envelope — never a
raw `-32602` (#307):

- A missing `confirm` (or `confirm: false`) →
  `confirm: true required for destructive tool system_restore_backup`.
  Note the word **destructive**, not **write**.
- A missing/empty **or** mismatched `confirmText` →
  `confirmText must match 'RESTORE <targetCompany>' exactly (got: '…')`.
  `confirmText` is schema-OPTIONAL precisely so that an omitted value
  reaches the handler and gets this envelope, instead of being rejected by
  SDK input validation with a `-32602`. Omission and mismatch are therefore
  indistinguishable from the agent's side: both are the same envelope.

`system_restore_backup` also accepts two optional verification-key paths,
mirroring the CLI's `--verify-key` / `--public-key`:

- `verifyKey` — path to the **symmetric HMAC** verification key
  (`.backup-manifest.key`). Verifies the manifest's HMAC tag.
- `publicKey` — path to the **asymmetric ed25519 public key**. Verifies the
  manifest's ed25519 signature, letting a third party check authenticity
  without the HMAC key.

They are distinct keys; do not pass an ed25519 `.pub` file as `verifyKey`.

## Ordering — read before you write

There is no enforced phase order, but the correct shape of work is
**read → write → verify**:

1. **Read to establish preconditions.** Before issuing an invoice, validate
   the payload (`invoice_validate`). Before posting one, check its status
   (`invoice_status`). Before booking an expense, list the bank transaction
   and the document. Reads are free and parallel — use them generously.
2. **Write with `confirm: true`.** A typical chain:
   `invoice_validate` → `invoice_issue` → `invoice_post` → later
   `invoice_settle_bank`. Or for manual bookkeeping: inspect with
   `journal_list`, then `journal_post`.
3. **Verify.** After a posting, `audit_verify` confirms the hash chain is
   intact; `journal_list` / `invoice_status` confirm the new state.

`audit_verify` åbner altid ledger'en read-only og fejler lukket ved ventende,
nyere, korrupt eller driftet schema. Schema-view-reparation er bevidst ikke et
MCP-tool; brug den actor-gatede CLI-kommando `system repair-schema-views`.

Some tools genuinely depend on earlier ones (you cannot `invoice_post`
before `invoice_issue`; you cannot `expense_book` without an ingested
document and an imported bank transaction). When a dependency is missing the
core says so in `errors[]` — see below.

## Preconditions and errors — where they live

Every tool answers with the envelope `{ ok, data?, errors[], appliedRules? }`.

- `ok: true` → `data` holds the payload; `appliedRules` lists the rule ids
  that fired for postings.
- `ok: false` → **a precondition was not met.** `errors[]` is a non-empty
  list of human-readable strings explaining exactly what. This is the
  contract: the agent does not need to know the preconditions up front — it
  attempts the call, reads `errors[]`, fixes the precondition, and retries.

Common precondition failures and the fix:

| `errors[]` says | Meaning | Fix |
|---|---|---|
| `confirm: true required for write tool …` | Write attempted without `confirm`. Envelope carries `code: "CONFIRM_REQUIRED"` — branch on that. | Re-call with `confirm: true`. |
| `confirm: true required for destructive tool system_restore_backup` | `system_restore_backup` attempted without `confirm`. The destructive tool says `destructive`, not `write` — but the `code` is the same `CONFIRM_REQUIRED`, so branch on `code` rather than the string. | Re-call with `confirm: true`. |
| `confirmText must match …` | `system_restore_backup` confirmText missing/empty **or** wrong (#307 — both cases give this envelope with `code: "CONFIRMTEXT_MISMATCH"`, never `-32602`). | Supply `RESTORE <targetCompany>` exactly. |
| balance / "går ikke i nul" | Journal entry debit ≠ credit. | Correct the lines so they balance. |
| `<field> <date> falls in <closed\|reported> period <kind> <start>..<end>` | Posting into a closed or reported period. | Post in an open period. Reopening a closed period is **CLI-only** — there is no MCP tool for it; surface it to the human to run `rentemester period reopen` (a controlled, audit-logged action; a `reported` period cannot be reopened). |
| VIES / VAT validation missing | EU customer not VAT-validated. | Run `customer_validate_vat` first. |
| `… låst …` (backup lock) | The opt-in bookkeeping lock is active. Envelope carries `code: "BACKUP_LOCKED"`. | Run `system_backup` with `archive:true`, then place it; see below. |

**Never guess past an `ok: false`.** The error is the precondition. If it
cannot be resolved deterministically, surface it to the human rather than
forcing the operation a different way.

One reply shape is **not** this envelope: a schema-invalid payload is
rejected by the MCP SDK before the handler runs, yielding a raw
`-32602` JSON-RPC error with `isError: true` and no `structuredContent` —
see "When the missing-`confirm` reply is *not* an envelope (`-32602`)" above.
Branch on that case before reading `errors[]`.

## Idempotency — retries are NOT automatically safe

- **Five high-risk bookkeeping writes accept `idempotencyKey` (maximum 128 characters):**
  `journal_post`, `journal_reverse`, `expense_book`, `payable_register` and
  `payable_pay`. The key requires an authenticated user or workspace service
  principal; its scope is the stable principal, workspace, company and operation
  — never the advisory actor or rotating credential. The key itself is hashed.
  Mutation, durable result and audit evidence commit atomically. An identical
  retry returns the original envelope with `data.idempotency.replayed: true`; a
  changed payload returns `IDEMPOTENCY_CONFLICT`. After 30 days response material
  may be pruned, but the permanent tombstone returns `IDEMPOTENCY_OUTCOME_EXPIRED`
  and can never be reused. Authorization, confirmation and policy gates still run
  before replay.
- Several tools *are* idempotent **by nature** (`annotations.idempotentHint`)
  — they de-dupe on content or period, not on a client key: intake polls
  (`mail_intake_ingest`, `imap_intake_poll`), `bank_import` (row-fingerprint
  de-duplication; see [`bank-import-idempotency.md`](bank-import-idempotency.md)), `recurring_invoice_generate` (per
  template/period), `invoice_render` (deterministic PDF) and
  `peppol_submit_public_invoice` (idempotent submission envelope).
  Re-running *these* produces no duplicate state. For every other write, the
  agent is responsible for retry-safety via read-back. The authoritative
  list is the live `annotations.idempotentHint` flags in `tools/list` — as
  of this writing exactly the reviewed tools above carry it.
- `invoice_send_email` is **unsafe-read-back**: SMTP acceptance is not a
  provider reconciliation contract. Read canonical delivery evidence before a
  retry; never infer that a duplicate-send is safe from the input alone.
- Writes without a supplied key retain their documented domain-specific retry
  behaviour; this contract never converts an unknown outcome into a safe retry.

## The append-only invariant

Rentemester's ledger is an append-only hash chain. There is **no delete and
no edit**. This shapes how an agent corrects mistakes:

- A wrong journal entry is fixed with `journal_reverse` (a counter-posting),
  not by deleting it.
- A wrong invoice is corrected with `invoice_credit_note`.
- A blocked or wrong intake/document lands in the exception queue; resolve
  it with `exception_resolve` (`exceptions_list` to inspect it first).

`journal_reverse` is itself a `write-irreversible` tool: it appends a new
counter-post; the chain only ever grows.

## The backup lock

Rentemester has an opt-in **bookkeeping lock** (`system_backup_lock`). When
enabled, new bookkeeping writes are refused if the weekly backup
(BEK 205/2024 § 4) is overdue beyond the grace period — the refusal envelope
carries the stable `code: "BACKUP_LOCKED"` (branch on that; the Danish
`errors[]` text containing `låst` is for humans). Backing up is the way
out: run `system_backup` with
`archive: true`, then place the archive on an EU/EEA destination
(`system_backup_place` / `system_backup_confirm_placement`). The
`system_*` and backup tools are never themselves blocked by the lock.

## Actor attribution

Every MCP call is attributed. The server derives the actor from the client's
`initialize` handshake (`agent:<client>/<version>`, optionally with a user
context) and passes it as an explicit parameter to the core — not via a
process env var, so parallel calls cannot collide. The actor is written into
`audit_log.actor`, giving a traceable chain from the agent call to the
append-only posting. See "Actor-attribution" in `docs/mcp-tool-surface.md`.

## Hard boundaries

- **The agent never overrules the ledger.** It posts only through these
  tools. When the core refuses (`ok: false`), that is final — record/relay
  it, do not route around it.
- **`confirm` is a decision, not a formality.** Set it only when committing
  to a write whose preconditions are met.
- **One company per company-scoped call.** Cross-company work is done by
  looping the single-company tools; nothing is consolidated across legal
  entities (this is also why `portfolio_overview` only juxtaposes).
- **DigiSense is profile-bound.** Use `efaktura_onboarding_status` to inspect
  readiness without secrets, then `efaktura_onboard` with `confirm:true` to
  register the ledger profile's CVR for both directions. Never reuse a
  `companyKey` from another ledger; the core rejects it before networking.
  For an active workspace, `efaktura_modtag_workspace` is confirm-gated and
  iterates every configured ledger itself; it accepts neither credentials nor
  a companyKey and returns redacted per-company outcomes.
- **Uncertain ⇒ exception or human, never a guess.** If a precondition
  cannot be resolved deterministically, surface it — the exception queue and
  the human are the contract for everything ambiguous.

## Relationship to the runtime agent contract

| | This contract (MCP loose tools) | `runtime-agent-contract.md` (`agent run`) |
|---|---|---|
| Driver | An external MCP client/agent | The in-process `runAgentLoop()` |
| Sequencing | Agent decides, per call | Fixed phase order (ingest→book→route→reconcile→deadlines→report) |
| State | None on the server; per-call | One company, one run |
| Use it for | Interactive / ad-hoc bookkeeping by an agent | Scheduled, deterministic, replayable bookkeeping runs |

Both rest on the same core, the same rules, the same append-only ledger and
the same exception queue. The guardrails are identical; only the driver
differs.
