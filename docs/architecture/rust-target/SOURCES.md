# Sources and interpretation

## User intent

BRIEF.md preserves the task. The three deployment experiences are user intent. Rust is the requested design direction for this proposal. GPUI, keeping React, a shared HTTP runtime, and migration sequencing are proposed technical choices. There is no claim of user acceptance of the detailed architecture.

## Existing implementation

The source-inventory.json records exact working-tree hashes and the base commit. Graphify was queried before raw source inspection; it was used as navigation, not imported as accepted intent. This bundle is a new target-design brief informed by selected code observations, not a lossless Graphify-to-GDD adapter conversion.

Verified observations:

- src/server/app.ts: Bun HTTP composition root; local/hosted split; hosted Better Auth dependency.
- app/src/lib/api/_shared.ts: relative fetch and same-origin credentials; a standalone remote desktop needs a different client transport/login lifecycle.
- src/core/paths.ts: each company has its own ledger.sqlite and original-document directories.
- src/core/money.ts: bigint-based scaled arithmetic already exists. Rust is not a remedy for an assumed absence of exact money handling.
- docs/deployment-modes.md: one deployment per organization, shared accounting rules, local lifecycle, hosted identity and credential-free transfer are existing design constraints. Its readiness claims are historical documentation, not freshly executed evidence.
- docs/security-multi-user-scope.md: existing authentication/authorization behavior is a migration obligation; documented production gates remain obligations.

## Method and UI references

- GDD: https://github.com/mikkelkrogsholm/gdd ; local clean checkout at 73494ed9933f5df7e663a0dec6a89aa29b10886a, CLI 0.8.0 development version, GDD 0.3 draft. Skill: /Users/mikkelfreltoftkrogsholm/Projekter/gdd/skills/gdd/SKILL.md.
- GPUI Kit: https://github.com/longbridge/gpui-kit ; README read in this conversation. No version pin or platform qualification has been performed. This is a candidate UI dependency, not an implemented dependency.

GDD sources are non-normative provenance. The graph is a proposed sketch. Existing behavior is not automatically correct: any diagnosed defect needs an explicit, separately tested change rather than silently becoming a compatibility requirement. No implementation evidence is recorded.
