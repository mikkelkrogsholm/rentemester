# Rust baseline capture (draft)

{
  "cli": 265,
  "mcp": 214,
  "http": 208,
  "browserCompany": 41,
  "browserStatic": 13,
  "coreFunctions": 734,
  "specialModules": 28,
  "migrations": 66,
  "sourceCoverage": 680,
  "drift": 0
}

- Draft inventory only; does not establish R01 completion.
- All exact per-operation HTTP/CLI schemas, error casing/nullability/streams, and MCP data payloads require explicit contracts and fixture verification.
- Core export inventory includes exported function declarations; exported arrow functions, object methods, classes, internal operations, schema-defined triggers and re-exports require a later pass.
- Browser static routes include wildcard and authentication gates; access conditions remain in App.tsx element source and are not semantically evaluated.
- Provider/job candidates are source-coverage/path-derived modules; no live credentials, procurement, scheduling or runtime availability verified.
- Owner R44/R45/R42/R46 is interface delivery ownership; domain ownership must be traced from core source coverage, not guessed from operation names.
- Original planning status retained separately; currentExistingBacklog is the fresh read-only GitHub snapshot and must be rebased before cutover.
- Registry presence indicates exposed capability, not implemented/complete runtime behavior.
- No per-operation positive oracle or negative fixture mapping yet.

Fresh read-only GitHub reconciliation: all 520 planning snapshot issue IDs found; 26 remain OPEN. The R01 named existing-work references currently are:

- #328 OPEN — Billy import: invoice line items for recreating invoices in Rentemester
- #329 OPEN — Billy import: VAT return history (momsindberetninger)
- #330 OPEN — Billy import: bank account metadata and reconciliation history
- #363 OPEN — EPIC: Bring-your-own Open Banking for sikker bank-sync på tværs af virksomheder
- #511 OPEN — Track website agent-discovery surface (.well-known, MCP card, modelContext)
- #512 OPEN — Mobilepay (GetPaid) integration - både short term (uafhængigt) og long term (partner)
- #524 OPEN — Chart of accounts: seed is one-size-fits-all — needs templates/profiles (holding ApS as concrete example)
- #525 OPEN — Backup destinations: add `proton-drive` as a first-class kind
- #543 OPEN — Google Workspace Gmail OAuth transport for the existing mail-intake pipeline
- #566 OPEN — Support attachmentless EML receipts as immutable accounting evidence
- #611 OPEN — Sikker reel e-mailafsendelse for fakturaer og recurring invoices via udskiftelig transport
- #612 OPEN — Importér allerede eksternt udstedte post-cutover-fakturaer uden genudstedelse eller gensendelse
- #624 OPEN — Hash-bundet metadata-supersession for ubogførte, ulinkede bilag
- #625 OPEN — Købsflow: særskilt pålagt moms som kostpris og reverse charge på dokumenteret nettoydelse
- #641 OPEN — Canonical company scope for company-knowledge MCP and cross-surface parity
- #642 OPEN — Prevent one journal entry from reconciling multiple bank transactions
- #643 OPEN — Bank import: Canonical source identity og fail-closed overlap før writes
- #664 OPEN — Restore: Bring runtime og dokumenteret overwrite-kontrakt i overensstemmelse

Extraction checks: every CLI key has a concrete spec source; every MCP tool has a concrete registration source, serializable input schema, and output schema. All 734 extracted core function declarations have a module migration owner. No company route view mapping is UNKNOWN. The 680 planning file hashes all match current source. This validates capture integrity, not runtime correctness or migration readiness.
