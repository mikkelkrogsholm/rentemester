# Rentemester build loop

Rentemester is built as a reliability loop: each iteration must improve either correctness, auditability, determinism, or real-world input handling. Avoid adding broad features unless the previous layer is trustworthy.

## Loop contract

Every loop starts with one narrow hypothesis and ends with a pushed commit or an explicit parked WIP branch/stash.

1. **Select target**
   - Prefer bugs, brittleness, audit gaps, real-world format gaps, or invariant leaks.
   - Write the hypothesis in one sentence, e.g. `quoted bank CSV text breaks import`.

2. **Classify risk**
   - `bugfix`: broken behavior, should go to `main` after green gates.
   - `hardening`: improves correctness/audit/test coverage, should go to short branch if multi-step.
   - `feature`: new capability, must have an acceptance fixture and no hidden ledger side effects.
   - `research`: no main commits; document findings or keep in branch.

3. **Write the failing proof first**
   - Add a unit, CLI, fixture, or smoke regression that fails for the target weakness.
   - For accounting behavior, include at least one negative case: what must be blocked.

4. **Patch minimally**
   - Keep changes inside the smallest core/module boundary.
   - Do not mix refactors with bookkeeping rule changes unless the refactor is required for the fix.
   - Prefer deterministic identifiers in examples and smoke flows.

5. **Verify in gates**
   - Run the focused test(s) first.
   - Then run `bun test`.
   - Then run `bun run smoke` on fresh `/tmp/rentemester-smoke`.
   - Run `git diff --check` before commit.

   **Smoke wall-clock budget**: smoke currently completes in ~2 seconds. To
   catch performance regressions before they bog down the build loop, CI uses
   `bun run smoke:budget`, which wraps `bun run smoke` and fails if wall-clock
   exceeds **30 seconds** (override locally with `SMOKE_BUDGET_SECONDS=<n>`).
   The plain `bun run smoke` target stays unchanged for fast local runs.

6. **Review the diff before commit**
   - Confirm no generated company data, secrets, temp files, or half-built features are included.
   - Confirm rules/docs/tests match the behavior.

7. **Commit and push**
   - Use one meaningful commit per loop.
   - Push only green work.
   - If a loop is not green, park it on a branch or named stash with notes.

## Claude Code issue intake

Claude Code may review the repository and create GitHub issues. Treat those issues as the primary external review queue for the build loop.

At the start of every automated or manual Rentemester loop:

1. Fetch open GitHub issues from `mikkelkrogsholm/rentemester`:

   ```bash
   gh issue list --repo mikkelkrogsholm/rentemester --state open --limit 30
   ```

2. Filter out PRs, duplicates, already-fixed reports, and speculative suggestions that need Mikkel's product decision.
3. Prioritize one narrow issue using this order:
   - ledger/audit integrity bugs
   - data loss or append-only violations
   - duplicate posting / bank-link bugs
   - deterministic reproducibility bugs
   - real-world import/parser bugs
   - VAT/accounting rule correctness
   - developer experience/refactor issues
4. Convert the issue into a failing test or fixture before patching.
5. Fix only that issue unless another issue is mechanically inseparable.
6. After green gates and push, close the issue with a comment containing:
   - commit hash
   - tests run
   - any follow-up issue created or remaining limitation
7. If an issue is invalid, duplicate, already fixed, or needs human decision, comment clearly and close or label/leave open as appropriate.

Do not let Claude Code issues create feature drift. Issues are sensors, not orders. The ledger invariants and green gates still decide what can land.

## Priority queue

Work the queue in this order unless Mikkel explicitly reprioritizes:

1. **Audit verify hardening**
   - Recompute hash chain from `GENESIS`.
   - Verify every journal entry balances.
   - Detect orphan journal lines and broken foreign keys.
   - Require document evidence for income/expense entries.
   - Detect duplicate bank settlement use.
   - Cross-check invoice status against ledger reality.

2. **Bank CSV hardening**
   - Support quoted fields, comma in text, semicolon delimiter, BOM, and useful header errors.
   - Add fixtures for realistic Danish bank exports.

3. **Deterministic smoke**
   - Remove hardcoded `document-id`/`entry-id` assumptions where possible.
   - Resolve by invoice number, bank reference/text/date/amount, and entry number.

4. **Money safety**
   - Centralize money arithmetic.
   - Move toward integer øre or a dedicated decimal helper.
   - Remove scattered `Number(...toFixed(2))`.

5. **Date and numbering correctness**
   - Derive entry/document/fiscal numbering from transaction/issue date or company fiscal year.
   - Do not use runtime year for accounting numbers.

6. **Exception queue**
   - Distinguish `safe`, `uncertain`, and `blocked` work.
   - Record reason, suggested action, required missing info, source evidence, and posting preview.

## Git strategy

### Main branch

`main` must always be runnable:

```bash
bun test
bun run smoke
```

Push to `main` only after both pass. Small docs-only commits can skip smoke only if they do not touch runtime files, but prefer running at least `bun test` when cheap.

### Branches

Use short-lived branches for anything that may take more than one loop:

```text
fix/<specific-bug>
harden/<area>
feature/<capability>
research/<question>
```

Examples:

```text
harden/audit-verify-hash-chain
fix/bank-csv-quotes
feature/bad-debt-recovery
```

### Commit format

Use imperative, outcome-oriented messages:

```text
Harden bank CSV import parsing
Verify ledger hash chain in audit
Add bad-debt recovery VAT postings
```

Each commit should contain:

- implementation
- tests/fixtures
- rule/docs update if behavior changes

### WIP handling

Do not leave accidental WIP on `main`. If interrupted:

```bash
git stash push -m 'wip <area>: <state>' -- <paths>
# or
git switch -c feature/<area>
git add -A && git commit -m 'WIP <area>'
```

Before switching tasks, run:

```bash
git status -sb
git diff --stat
```

### Release rhythm

For now: push every green loop. Later, add tags for usable checkpoints:

```text
v0.1-ledger-core
v0.2-invoice-lifecycle
v0.3-audit-export
```

## Issue closeout checklist

When a loop started from a GitHub issue, close it only after the fix is pushed. Use:

```bash
gh issue comment <number> --repo mikkelkrogsholm/rentemester --body "Fixed in <commit>. Gates: <focused test>, bun test, bun run smoke."
gh issue close <number> --repo mikkelkrogsholm/rentemester --comment "Closed after green gates and push."
```

If smoke is intentionally skipped for docs-only work, say so explicitly. Never close an accounting/runtime issue without a regression test.

## Definition of done

A loop is done only when:

- focused regression passes
- `bun test` passes
- `bun run smoke` passes, unless docs-only
- `git diff --check` passes
- commit is pushed or WIP is explicitly parked
