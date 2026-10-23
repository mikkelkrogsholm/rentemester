# R01 migration contract inventory

This inventory fixes the source contract at `8866e33b49023a02e8f82fe9e69b5e8d2a412b7e`.
It completes the static inventory responsibility of [#673](https://github.com/mikkelkrogsholm/rentemester/issues/673).
It does not mark the remaining Rust implementation, runtime qualification,
provider access, distribution, cutover or installed desktop acceptance complete.
One shared Rust engine/server and the same React UI in web and Tauri remain binding.

The original [inventory.json](inventory.json) and its original source revision
`0bbe82bcf0cafd73e2326152740cb06b90222405` remain unchanged. The new
[current-source.json](current-source.json) is the exact current registry capture;
its draft extractor fields are superseded by the concrete contract tables below.
Do not use a registry count or module-coverage PASS as behavior qualification.

| Contract artifact | Covered source behavior |
| --- | --- |
| [contracts-cli.json](contracts-cli.json) | All 265 CLI keys, concrete flags/dispatch/producer results, human/JSON rendering, actor/startup/parse boundaries, streams and exit behavior |
| [contracts-mcp.json](contracts-mcp.json) | All 214 tools, Zod inputs, exact result constructors, confirmation/security/schema refusals, text versus structured content |
| [contracts-http-ui.json](contracts-http-ui.json) | All 208 HTTP routes and 55 unique screen paths: 42 company screens and 13 static paths from 14 contextual registrations; recursive child/API/provider and global setup/MFA/cockpit activity |
| [contracts-core.json](contracts-core.json) | 269 coherent internal/schema/release behavior rows; every 738 exported function declaration, 67 migration artifacts and 28 provider/job candidates indexed; arrows/constants/methods/re-exports and SQL guards reviewed explicitly |

Every row identifies its real business owner among the original 55 issues,
applicable interface, source-supported/partial/unavailable/roadmap/corrected status,
input, success, refusal and parse/protocol contract, field casing/nullability,
stdout/stderr/exit/MCP applicability, oracle and negative case. Transport ownership
is distinguished from business ownership. Coherent internal helpers and type-only
barrels do not become newly exposed product features.

Exact source signatures alone are not independent acceptance oracles. Existing
checked-in test fixtures/assertions are linked as **source-reviewed specifications**;
other rows state explicit synthetic expected amounts, identities, schema or state
transitions as **planned** acceptance. Neither label means a test was run here.
Runtime execution remains with R08 and the relevant original domain/interface issue.
Known corrections in [the correction register](../known-contract-corrections.md)
and [TRIAL_BALANCE.md](TRIAL_BALANCE.md)/[trial-balance-wire.json](trial-balance-wire.json)
take precedence over old tests that reproduce a known defect.

Review corrected the CLI's erroneous classification of `console.error` and
`process.exit` as stdout, expanded the HTTP compiler-`any` result boundaries into
actual constructors/SQL projections, retained the LoginView/NotFound contextual
wildcard, and included global pre-route setup/MFA and nested screen effects.
Legacy batch SQL JSON strings/nullable joins/SQLite integer flags remain distinct
from native typed results. The old v1 Bun release manifest/approval contract and
native v2 identity are both retained. Native CLI candidate assembly remains a
candidate operation; it does not install, publish or qualify the app.

[historical-dispositions.json](historical-dispositions.json) resolves all 18 named
original issues. #641 is actually closed; the other 17 retain their observed status
and explicit delivered/correction/pending boundaries. The full
[existing-backlog-reconciliation.json](existing-backlog-reconciliation.json) records
all 520 historical planning issues: 495 closed and 25 open at the recorded query.
Those are historical backlog states, **not** the Rust epic's completion count.
Open Billy/Gmail/Proton/MobilePay/website/attachmentless-email/chart-template wishes
are not silently imported as new rewrite scope or declared delivered.

[source-delta-review.json](source-delta-review.json) covers all 295 changed source
files since the original source revision, including the 103 changed paths present
in the narrower original planning hash set. The
[UI detail](ui-source-delta-review.json) maps all 277 UI paths (92 tests) to concrete
behavior, domain ownership, shared infrastructure or a reasoned type/test boundary.
The other 18 changes explicitly preserve retirement fences, atomic normalization,
retention dates/large identifiers, schema guards, mail intake and release identity.
Hashes bind the exact original/current bytes and actual diffs. A source change at
a later cutover candidate requires a new explicit delta review; this snapshot must
not silently float to HEAD.

Run the bounded, read-only integrity check from the repository root:

```sh
python3 scripts/rust/verify-baseline-contracts.py
```

It verifies the frozen Git objects, coverage, owners, contract/oracle/refusal fields,
reference resolution, historical scope and source deltas. Dropping an operation or
tampering with an anchor must fail. It reads source references from their frozen
revision, so later evidence documentation does not invalidate an older receipt.
The check cannot replace semantic review or prove that Rust implements a contract.
The four SGD partitions were reviewed against actual source before integration;
failed initial deliveries and bounded repairs remain in their original run receipts.
No provider request, company migration, auth setup, invoice send or accounting action
was replayed to make this inventory.
