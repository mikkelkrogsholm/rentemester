# Graphify

Rentemester has a Graphify knowledge graph in `graphify-out/graph.json`.
The rebuild workflow uses Graphify 0.9.74: local AST extraction for
code and SQL, plus semantic extraction of documentation, rules, example
documents, and images through Codex agents. Luna performs the initial semantic
pass, with Sol repairing document chunks that need more detail.

Use it before broad codebase searches:

```bash
graphify query "how does invoice issuing connect to ledger posting?"
graphify explain "postIssuedInvoiceToLedger"
graphify path "issueInvoice()" "postJournalEntry()"
```

Refresh code relationships locally without an LLM:

```bash
graphify update . --no-cluster
```

For a full semantic rebuild inside Codex, invoke the Graphify skill on the
project. The skill can use the session's agents without a separate API key.
Code updates do not semantically re-read changed documentation; run the full
skill again when that material needs refreshing.

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

The interactive view is `graphify-out/graph.html`; `GRAPH_REPORT.md` describes
communities and important connections. Local Git hooks, if configured, are
machine state rather than committed repository files. Check their status with
`graphify hook status`.

Build provenance, source fingerprints and limitations are recorded in
`graphify-out/build-info.json`; graph diagnostics are in
`graphify-out/graph-health.json`. Native-agent token usage may be unavailable;
this must be reported explicitly rather than as zero semantic cost.
