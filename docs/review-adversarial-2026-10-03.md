# Rentemester review and adversarial review — 2026-10-03

Requested review candidate: `270e94e076ef8bca303c64c2ee1c88d1b5cdb2c8`
on `codex/rentemester-simplification`, draft PR #667. Four read-only reviewers
covered security/interfaces, ledger/domain, operations/imports/backups and
cockpit/runtime. The lead reproduced supported triggers, made the repairs and
added regressions; the reviewers then checked the changed surfaces again.

This is a finite source and adversarial scenario review, not proof that every
possible defect is absent. It preserves the general product boundary and uses
synthetic company data. No schema change, merge or deployment is included.

## Repaired findings

| ID | Demonstrated trigger and effect | Repair and observable regression |
| --- | --- | --- |
| A1 | Equal high-confidence purchase matches arbitrarily selected the first document and posted it. | A unique strongest match is required; ties create `AGENT_AMBIGUOUS_MATCH` with candidate evidence. Repeated runs do not post; a uniquely corroborated match still posts. |
| A2 | Journal JSON with non-array lines, null lines, numeric currency or rounding overflow threw or leaked invalid numeric values. | Structured validation refuses malformed input before effects. CLI denial, unchanged audit/sequences and the next valid entry are tested. |
| A3 | Asset depreciation/write-off could be dated before acquisition. | Both operations reject pre-acquisition dates without posting; acquisition-day posting remains supported. |
| A4 | Duplicate bank references selected the newest row; an ID plus conflicting reference silently ignored the reference. | One selector serves invoice payment, refund and claim payment. Ambiguous references and conflicting selectors reject without effects; an explicit matching ID succeeds. |
| A5 | A registered company symlink could resolve outside the workspace or alias another company through the legacy slug resolver. | Shared canonical direct-child containment rejects escape, alias and dangling symlinks. Resolver and live HTTP route tests pin denial. |
| A6 | Successful, rejected and throwing ZIP parsers left private extraction directories behind. | The import owner removes ZIP roots in a finally block; source-resolution failures also clean up. Caller ZIP and directory inputs remain intact. |
| A7 | Backup signed success despite missing/tampered document bytes, and could include unregistered orphan files. | The frozen document register and copied evidence must match in both directions, with hashes and nonempty bytes. Denial removes owned staging and emits no success audit. Restore checks the same correspondence. |
| A8 | Group disposition proposals were persisted before company actor authorization. | Read-only planning and both-company authorization precede the writer; unauthorized calls leave proposals, events and workspace audit unchanged. |
| A9 | Six knowledge/ownership CLI writes bypassed company allowlists; HTTP/MCP review/apply could take permission scope from caller-supplied facts. | Explicit and derived actors pass every endpoint policy. Stored assertions/snapshots determine review/apply scope. Intruder, partial scope and decoy facts reject before effects; authorized lifecycle succeeds. The stale CLI documentation was corrected. |
| A10 | A late accountant-export response could download the previous company's data after navigation/unmount. | The requesting view owns a lifetime token; abandoned views discard late bytes. A current-view download still occurs once and releases its object URL. |
| A11 | An already-open second browser tab could repeat a write whose result was unknown in the first tab. | A verified shared blocker precedes each guarded request; change notifications and fresh pre-write reads preserve blockers across tabs and reloads. Hosted blockers use stable user scope. Unowned legacy blockers remain a shared quarantine until explicit reconciliation; storage failure before sending prevents the request. A two-page browser scenario pins one write. |

## Acceptance and limits

`graphify-out/review-verification.json` records the final integrated gate,
source fingerprints and exact suite counts. The gate includes backend and
cockpit tests, Chromium/WebKit scenarios, runtime/app typing, lint, discovery,
supply-chain checks, PDF benchmark, CLI/MCP smoke, local container integration
and reproducible image packaging. Focused regressions cover each repaired
finding and positive supported behavior.

The browser guard handles unknown results across tabs sharing an origin and
user; it does not provide server-side exactly-once execution, coordinate other
devices, or prevent two writes started simultaneously racing to claim the same operation. Human reconciliation is still required to release an unknown-result
blocker. External providers and a production hosted authentication deployment
were not exercised end-to-end; hosted permissions were exercised with local
Better Auth service-principal fixtures.

The graph's AST relationships are refreshed after the repairs. Retained
semantic fragments have their original evidence quality; changed documentation
is identified separately in the new graph provenance, without claiming a new
full semantic extraction.
