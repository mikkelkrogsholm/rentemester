# Billy sync: bilag-vedhæftning ved inkrementel sync

**Dato:** 2026-06-12
**Status:** Godkendt (tilgang A — hybrid)

## Problem

`scripts/billy-sync.ts` spejler nye Billy-posteringer som journal-entries, men henter
ikke bilag. `verifyAuditChain` kræver `document_id` på indtægts-/udgiftsentries
(waiver findes kun for `rentemester-import-postings`), så alle billy-sync-entries
flages som "missing document evidence". Ledgeren er append-only, så `document_id`
kan ikke tilføjes bagefter — og bilag vedhæftes ofte i Billy *efter* transaktionen
er oprettet.

## Løsning (hybrid)

1. **Bilag før bogføring:** Ved sync hentes transaktionens bilag fra Billy.
   Findes bilag, ingestes det og entry'en bogføres med `document_id` — ægte
   evidens uden waiver.
2. **Waiver for resten:** `billy-sync` tilføjes til `IMPORTED_HISTORICAL_PROGRAMS`
   i `src/core/ledger.ts`, så entries uden bilag (endnu) ikke fejler audit —
   samme model som den historiske import.
3. **Backfill:** Senere sync-kørsler opdager nye bilag på allerede-syncede
   transaktioner og linker dem via `import_document_links` (mønstret fra
   `billy-bilag.ts`). Dagens 15 flagede entries backfilles samme vej.

## Mapping (uden heuristik)

Billy `/transactions` leverer `originatorReference` på formen `{type}:{id}`
(fx `bill:<billId>`, `invoice:<id>`). Attachments har `ownerId` = samme id.
Kæden er altså direkte: `transactionId → originatorReference → ownerId →
attachments[]` — ingen dato+beløb-matching.

## Komponenter

### `src/core/import/billy-sync-bilag.ts` (ny — testbar kernelogik)
- `ingestBilagFile(db, companyRoot, filePath)` → `{documentId, documentNo, deduped}`.
  SHA-256-dedup mod `documents` før `ingestDocument` (som `billy-bilag.ts`).
  Source: `billy-sync-bilag`, documentType: `cash_register_receipt`.
- `linkBilagDocument(db, transactionId, documentId, journalEntryId)` — idempotent
  insert i `import_document_links` (source_system `billy`).
- `matchEntryToBillyTxn(entries, billyTxns)` — backfill-matcher for entries
  posteret FØR denne feature (ingen gemt txn-mapping): match på
  `transaction_date` + total debet-beløb + tekst (`Billy sync: {txt}`).
  Tvetydige matches springes over og rapporteres.

### `src/core/ledger.ts` (ændring)
- `IMPORTED_HISTORICAL_PROGRAMS` udvides med `"billy-sync"`.

### `scripts/billy-sync.ts` (udvidelse)
- Hent `/transactions` (pagineret) → `txnId → originatorReference`.
- Hent `/attachments` (pagineret) → `ownerId → AttachmentMeta[]`.
- Pr. ny transaktion: download bilag-filer (via `/files/{fileId}` → `downloadUrl`,
  rate-limit: 200 ms pause pr. 10 kald — samme som `billy-export.ts`) til
  `{company}/sync/billy-bilag/{ownerId}__{attachmentId}.{ext}`.
  Første bilag → `documentId` på `postJournalEntry`; øvrige → `linkBilagDocument`.
- Sync-state udvides med `pendingBilag: [{txnId, entryId, entryNo, ownerRef}]` —
  transaktioner bogført uden bilag. Fjernes fra listen når bilag linkes.
- Backfill-trin pr. kørsel: (a) `pendingBilag`-listen, (b) éngangs-rekonstruktion
  af gamle billy-sync-entries uden dokument/link via `matchEntryToBillyTxn`.
- `--dry-run` rapporterer hvilke transaktioner der har bilag klar.

## Fejlhåndtering

- Download-fejl på et bilag: entry bogføres alligevel (uden `documentId`),
  transaktionen lægges i `pendingBilag` og forsøges igen næste kørsel.
- Tom fil / manglende `downloadUrl`: tælles som fejl, springes over.
- Backfill-match tvetydig: spring over, rapportér — bogfør aldrig på gæt.

## Afgrænsning

- Salgsfakturaers PDF (Billy-genererede) hentes IKKE — kun attachments
  (typisk udgiftsbilag). Indtægtsentries uden bilag dækkes af waiveren.
  Kan tilføjes senere hvis ønsket.
- Ingen ændring af det historiske import-flow (`billy-bilag.ts`).

## Test

- Unit-tests for `billy-sync-bilag.ts`: ingest m. dedup, idempotent link,
  backfill-matcher (entydig, tvetydig, intet match).
- Eksisterende audit-test udvides: `billy-sync`-entry uden dokument passerer
  audit; med `document_id` valideres dokumentet.
- Manuel verifikation mod live Billy-data (dry-run + rigtig kørsel +
  `audit_verify`).
