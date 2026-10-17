# Rentemester Rust migration plan

GitHub epic: https://github.com/mikkelkrogsholm/rentemester/issues/672

The user requested a comprehensive epic and child issues, complete local planning first, repeated review/fix and then GitHub publication. This package covers that task; it does not execute the rewrite or authorize a live accounting migration.

## Navigate

- EPIC.md: full product outcome, boundaries, phases, completion criteria and named owner journeys.
- plan.json: canonical issue definitions, 218 completion-dependency edges, seven named owner journeys and source anchors.
- issues/: 55 self-contained issue bodies; actual GitHub references are rendered from github-map.json.
- gdd.json and graph.html: derived GDD delivery model and offline interactive graph.
- source-coverage.csv: 680 source/rule/surface files mapped to initial delivery owners with exact hashes. This is module-level traceability, not a substitute for R01's per-operation inventory.
- REVIEW.md and reviews/: three review/fix rounds; all 13 findings closed within their review scopes.
- checks.json, model-report.json and view-report.json: actual planning checks. Earlier round outputs are preserved as history.
- publication-ready.json: reviewed local plan/body fingerprints recorded before any GitHub issue creation.
- github-map.json and publication-report.json: resumable actual issue identities and final read-back verification (the final report appears after publication completes).

## Execution sequence

P0 establishes baseline, GDD contracts and binding compatibility/identity/API/native decisions. P1 delivers the first read-only Rust trial balance. P2 establishes ownership, evidence, authority and atomic writes. P3 ports accounting domains. P4 implements provider/identity/runtime boundaries. P5 completes CLI/MCP/React/GPUI. P6 verifies parity, packages exact bytes, rehearses fenced cutover and retires the TypeScript backend.

Dependency edges describe completion, not an unnecessary ban on contract-driven parallel scaffolding. Ready scopes can proceed independently with stable contracts and explicit resource ownership; shared-checkout reviewers are read-only. No future run or implementation agent is scheduled by this planning package.

## Verification limits

Model/view/DAG/source checks passed. These results establish plan validity and traceability only. Delivery actions remain sketches until refined per slice, and planned acceptance scenarios have not been executed against a Rust product. Signing credentials, platform qualification, external security/provider review and all existing production gates remain explicit future prerequisites.
