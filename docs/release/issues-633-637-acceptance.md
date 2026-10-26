# Acceptance evidence for issues 633–637

All evidence below uses synthetic fixtures. The implementation does not add a
schema migration; the ledger schema remains version 51.

## #633 — stale purchase sources and reassessment

| Requirement | Evidence |
| --- | --- |
| Get, overview and readiness use the same stale status | `purchase-cases.test.ts`: “uses document metadata changes and missing sources in the same stale-status contract”; “surfaces source-bound documentation outcomes in period close…” |
| Exact append-only reassessment; stale version/hash rejected | `purchase-cases.test.ts`: “requires an explicit append-only reassessment…” |
| Missing source remains a concrete close blocker | `purchase-cases.test.ts`: “uses document metadata changes and missing sources…” |
| Current policy, actor/principal separation and no ledger/VAT mutation | `accounting-approval-policy.test.ts`: “binds a purchase review to the exact current policy…”; `purchase-cases.test.ts`: “groups only exact unresolved cases…” |
| CLI, MCP and HTTP parity | `purchase-case-cli-lifecycle.test.ts`: “shows stale source evidence, reassesses the exact version, and updates period readiness”; `mcp-purchase-case-lifecycle.test.ts`: “requires explicit reassessment…”; `server-purchase-cases.test.ts`: authenticated reassessment/readback |
| Discovery coverage | `agent-discovery-coverage.test.ts` plus the release `agent-discovery:coverage` gate |

## #634 — provisional purchase economics

| Requirement | Evidence |
| --- | --- |
| DKK 1,250 gross / 1,000 expense / 250 expected VAT before posting | `purchase-cases.test.ts`: “projects one known unposted DKK draft…” |
| Document and bank links do not multiply one draft | Same test; one draft reports both case IDs but one economic effect |
| Two active drafts for one source fail closed | `purchase-cases.test.ts`: “does not count two active drafts for the same canonical source” |
| Posted source is not counted provisionally; posting preserves combined effect | `purchase-cases.test.ts`: “does not count an active draft when its source already has a canonical posting”; “moves a reviewed draft…” |
| Normal payable followed by bank payment counts the expense once | `purchase-cases.test.ts`: “counts a normal payable purchase once before and after its later bank payment” |
| No-case history is visible without backfill | `purchase-cases.test.ts`: “uses the draft economic date and shows a draft without inventing a historical case”; documented in `docs/mcp-tool-surface.md` |
| Unknown VAT and invalid accounts fail closed; balance lines are not expenses | `purchase-cases.test.ts`: “does not invent expense or VAT for balance transfers and missing VAT classification”; accounting-draft validation is covered by `accounting-drafts.test.ts` |
| FX exclusion, documented conversion, credit sign and provenance | `purchase-cases.test.ts`: “uses documented DKK conversion and preserves credit-note signs and draft provenance” |
| Deterministic company isolation | Stable `sourceHash` assertions in purchase-case tests; hosted cross-company access denial in `server-api/better-auth-rbac.test.ts` and `mcp-service-security.test.ts` |

## #635 — purchase cockpit

| Requirement | Evidence |
| --- | --- |
| Concrete date, supplier/unknown, amount/currency, document link and statuses | `PurchaseOverviewView.test.tsx`: “shows the provisional projection as not filing-ready and keeps it optional” |
| Open from readable existing context | `PurchaseOverviewView.test.tsx`: “prefills a readable source selected from an existing context link” |
| Exact selected group members | `PurchaseOverviewView.test.tsx`: “group review sends only the concretely selected members”; core stale preflight in `purchase-cases.test.ts` |
| Individual ordinary/alternative review and stale reassessment | `PurchaseOverviewView.test.tsx`: “reviews one current case…” and “requires a reason and sends the fresh fingerprint…” |
| Optional projection and expected-VAT warning | `PurchaseOverviewView.test.tsx`: projection/toggle test; it also asserts zero POST requests from the toggle |
| Permission/error does not show false success | `PurchaseOverviewView.test.tsx`: “keeps the exact review open when the server denies permission” |
| Existing navigation only | Context links originate in the existing bank and document views; no new administration area was added |

## #636 — approval policy

| Requirement | Evidence |
| --- | --- |
| Normal sole bookkeeper remains supported | `accounting-approval-policy.test.ts`: “allows a sole authorized bookkeeper only through scoped membership…” |
| New elevated activation is rejected with a stable code | `server-accounting-approval-policy.test.ts`: HTTP and real CLI tests; `mcp-purchase-case-lifecycle.test.ts`: MCP test |
| Historical elevated evidence is preserved and labelled not enforced | `accounting-approval-policy.test.ts`: “retains historical elevated policy evidence…” |
| Policy changes, actor/principal confusion and membership fail closed | `accounting-approval-policy.test.ts`: exact-policy, principal and scoped-membership tests |
| Real callers and transports are covered | Purchase-case and accounting-draft caller tests, CLI, HTTP and black-box MCP tests listed above |

## #637 — atomic group review

| Requirement | Evidence |
| --- | --- |
| Case-event fault rolls back every table | `purchase-cases.test.ts`: “rolls back an injected group-review write failure…” |
| Member-link and audit faults roll back every table | `purchase-cases.test.ts`: the two corresponding rollback tests |
| Stable bounded domain error | The three fault tests assert `PURCHASE_CASE_GROUP_WRITE_REJECTED` |
| Success writes one group and one review per member | `purchase-cases.test.ts`: “groups only exact unresolved cases…” |
| Same-key retry has no additional business writes | `purchase-cases.test.ts`: “replays a successful group review…” |
| Stale member fails before the first write | `purchase-cases.test.ts`: “groups only exact unresolved cases and preflights stale members before any write” |
