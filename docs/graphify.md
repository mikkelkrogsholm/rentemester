# Graphify

Rentemester has a Graphify knowledge graph in `graphify-out/graph.json`.
The graph combines local AST extraction for code and SQL with semantic
extraction of documentation, rules, example documents, and images through
Codex agents. The original semantic rebuild used Luna, with Sol repairing
document chunks that needed more detail. Build metadata records the versions
and extraction limits for each run.

## Required agent workflow

Graphify must be used first for codebase questions, analysis, development,
debugging, reviews, adversarial reviews, and refactoring. This applies to
subagents as well as the main agent; no explicit `/graphify` request is needed.
`AGENTS.md` contains the binding project instructions.

Run queries from the checkout where the work is happening, before raw code
searches or inspection, then verify the relevant findings in the source:

```bash
graphify query "how does invoice issuing connect to ledger posting?"
graphify explain "postIssuedInvoiceToLedger"
graphify path "issueInvoice()" "postJournalEntry()"
```

Refresh code relationships locally without an LLM:

```bash
graphify update .
```

Run this explicitly before committing code changes and before finishing the
task, including in linked worktrees. A read-only subagent leaves the update to
the main agent. Dirty graph output is expected and does not waive the query
or update requirements.

## Semantic updates

After changing documents, rules, or images, use the Graphify skill's incremental
semantic workflow on the changed sources. It can use the session's agents
without a separate API key. Preserve the existing semantic layer and record
any pending sources in build metadata. An AST update does not semantically
re-read changed documentation.

The headless CLI instead uses a configured LLM backend:

```bash
graphify extract .
```

Pass `--force` for an intentional full replacement. Upgrade the CLI, including
the SQL parser needed for the ledger schema, and refresh the installed skill:

```bash
uv tool install --force 'graphifyy[sql]@latest'
graphify install --platform codex
graphify install --platform agents
```

## Git hooks and tool guards

Install and check the native Git hooks once per local repository:

```bash
graphify hook install
graphify hook status
```

Installation preserves existing hooks, registers `post-commit` and
`post-checkout`, and configures the Graphify merge driver. The committed
`.gitattributes` entry associates `graphify-out/graph.json` with that driver;
the driver command and executable hooks are local Git configuration and must
be installed on each new machine or clone.

In the primary checkout, these hooks launch a background AST refresh after
commits and branch changes. The current Graphify implementation deliberately
skips linked worktrees, so agents must run `graphify update .` explicitly in
those worktrees. Hook status confirms installation, not completion. Check
`~/.cache/graphify-rebuild.log` for completion or errors. Git hooks do not
perform semantic extraction and do not enforce query-first behavior.

Claude Code's project settings use `graphify hook-guard search` for `Bash|Grep`
and `graphify hook-guard read --strict` for `Read|Glob`. The strict read guard
blocks the first raw read per session when applicable; it is not permanent
enforcement of every tool call. The agent instructions remain authoritative.

Codex uses `AGENTS.md` and the Graphify skill for the same requirements.
Graphify's `hook-check` command is intentionally a no-op in the installed
Codex integration, so the obsolete `.codex/hooks.json` entry is removed rather
than presented as an enforcement mechanism.

The interactive view is `graphify-out/graph.html`; `GRAPH_REPORT.md` describes
communities and important connections. Prefer `query`, `path`, `explain`, and
the wiki for scoped navigation before reading the full report.

Build provenance, source fingerprints and limitations are recorded in
`graphify-out/build-info.json`; graph diagnostics are in
`graphify-out/graph-health.json`. Native-agent token usage may be unavailable;
this must be reported explicitly rather than as zero semantic cost.
