# MCP Tool Surface — Rentemester

Spec for hvordan Rentemesters CLI-kommandoer eksponeres som MCP-tools til
agenter (Claude, Cursor, Claude Code, Codex osv.). Dokumentet er bygge-tegning
for MCP-epicen (#89) og forudsætning for scaffold (#77), implementation (#78)
og demo (#79).

Kilder:
- `src/cli-meta.ts` — den autoritative liste af CLI-kommandoer (49 kommandoer).
- `src/core/*.ts` — TypeScript-typer for inputs og resultater (`InvoicePayload`,
  `JournalEntryInput`, `BankImportRow`, `DocumentMetadata`, `ActorContext` osv.).
- `src/cli-format.ts` — output-konventionen `{ ok, errors, ... }` som vi
  genbruger til MCP-svar.

## Designprincipper

1. **En MCP-tool = præcis én CLI-kommando.** Tools navngives `snake_case` med
   første led som domæne (`invoice_*`, `bank_*`, `journal_*`, `documents_*`,
   `system_*`, `vat_*`, `customer_*`, `vendor_*`, `period_*`, `retention_*`,
   `exceptions_*`, `accounts_*`, `reconcile_*`, `expense_*`, `audit_*`). Dette
   matcher CLI'ens `domæne underkommando`-struktur 1:1 og gør det trivielt at
   regenerere registret fra `COMMAND_SPECS`.
2. **Typed inputs via zod genereret fra TypeScript-typerne.** For hver tool
   defineres en `z.object({...})`. Hvor kernen allerede har en type
   (`InvoicePayload`, `JournalEntryInput`, `BankImportRow`,
   `DocumentMetadata`, `CreateCustomerInput`, `CreateVendorInput`,
   `BookExpenseFromBankInput`, `CloseAccountingPeriodInput`,
   `ReverseChargePurchaseInput`, `RepresentationPurchaseInput`,
   `ExportAuthorityPackageInput`, `RestoreSystemBackupInput`,
   `RecordExceptionInput`, `ResolveExceptionInput`) genereres zod-skemaet
   parallelt og holdes synkroniseret via en unit-test (separat task — se
   Forudsætninger).
3. **Struktureret output `{ ok, data?, errors[], appliedRules? }`.** Vi
   genbruger kernens eksisterende `JournalPostResult`/`*Result`-shape og
   wrapper alle outputs i et fælles convolut. `ok=true` ⇒ `data` er sat;
   `ok=false` ⇒ `errors` er en ikke-tom string-liste. `appliedRules` listes
   altid for kommandoer der bogfører (sporbarhed mod regelsæt).
4. **Sikkerhedsklassifikation** på fire niveauer:
   - `read` — ingen state-bivirkninger; agenten må kalde frit og parallelt.
   - `write-reversible` — opretter state der kan tilbageføres via
     `journal_reverse`, `invoice_credit_note`, eller `exception_resolve`.
   - `write-irreversible` — bogfører i append-only kæde (audit_log + hash);
     kan kun "rulles tilbage" via en modpostering. Kræver `confirm: true`.
   - `destructive` — system-niveau (restore, retention purge, backup-rotation).
     Kræver `confirm: true` **og** `confirmText: "<præcis fritekst>"`.
5. **Actor-attribution er obligatorisk.** Hvert MCP-call sætter
   `RENTEMESTER_ACTOR=agent:<client-info>` (jf. #63) før kerne-funktionen
   kaldes. Format: `agent:claude-code/0.4.1 (user:mikkel@56n.dk)`.
   `auditActor` skrives ind i `audit_log.actor` og udgør traceable kæde fra
   agent-call til bogføring.
6. **Idempotency-keys på alle writes.** Klienten kan sende
   `idempotencyKey: "<uuid>"` i input. Serveren cacher senest succesfulde
   svar i 24h og returnerer samme svar ved gen-kald — beskytter mod
   dobbelt-bogføring ved netværks-retry.
7. **Eksplicit `company`-parameter overalt.** Aldrig implicit "current
   company"; agent skal altid pege på den absolutte sti. Forhindrer
   utilsigtet cross-company-skade.

## Klassifikation

| Niveau | Krav | Eksempler |
|---|---|---|
| `read` | Ingen | `audit_verify`, `bank_list`, `invoice_status`, `vat_report` |
| `write-reversible` | `confirm: true` | `customer_create`, `vendor_create`, `bank_import`, `documents_ingest`, `exception_resolve` |
| `write-irreversible` | `confirm: true` | `journal_post`, `invoice_issue`, `invoice_post`, `invoice_settle_bank`, `expense_book`, `vat_post_*` |
| `destructive` | `confirm: true` + `confirmText` | `system_restore_backup`, fremtidige retention-purges |

`journal_reverse` er klassificeret som `write-irreversible`: den skriver en ny
post i den append-only kæde — den modposterer en tidligere post, men kæden
selv ændres ikke.

## Read-tools

| Tool | CLI-ækvivalent | Input | Output | Brief |
|---|---|---|---|---|
| `audit_verify` | `audit verify` | `{ company }` | `{ entries, ok, errors[] }` | Verificerer hash-chain og bogføringsintegritet. |
| `accounts_list` | `accounts list` | `{ company }` | `{ accounts: AccountRow[] }` | Lister kontoplanen. |
| `bank_list` | `bank list` | `{ company, status?, from?, to?, textMatch?, amount? }` | `{ transactions: BankTransactionRow[] }` | Lister importerede banktransaktioner med filtre. |
| `bank_suggest_matches` | `bank suggest-matches` | `{ company, bankTransactionId?, max? }` | `{ suggestions: BankMatchSuggestion[] }` | Foreslår deterministiske match mellem uafstemte bank-poster og bilag. |
| `customer_list` | `customer list` | `{ company, archived? }` | `{ customers: CustomerRecord[] }` | Lister kendte kunder. |
| `customer_validate_vat` | `customer validate-vat` | `{ company, cvr }` | `{ valid, cachedAt, name?, address? }` | Validerer EU-VAT via VIES og cacher resultatet. |
| `documents_list` | `documents list` | `{ company }` | `{ documents: DocumentRow[] }` | Lister gemte bilag. |
| `exceptions_list` | `exceptions list` | `{ company, status? }` | `{ exceptions: ExceptionRow[] }` | Lister exceptions-køen (open/resolved/all). |
| `invoice_status` | `invoice status` | `{ company, documentId? | invoiceNumber?, asOf? }` | `{ status, openBalance, paidAmount, ... }` | Viser åben saldo og status på en faktura. |
| `invoice_list` | `invoice list` | `{ company, status?, from?, to?, customerCvr?, customer?, invoiceNumber?, minAmount?, maxAmount?, asOf? }` | `{ invoices: IssuedInvoiceRow[] }` | Lister udstedte fakturaer med filtre. |
| `invoice_find` | `invoice find` | `{ company, query?, customer?, amount?, invoiceNumber?, asOf? }` | `{ matches: IssuedInvoiceRow[] }` | Søger efter fakturaer på nummer, kunde eller beløb. |
| `invoice_overdue` | `invoice overdue` | `{ company, asOf?, minDays? }` | `{ invoices: IssuedInvoiceRow[] }` | Lister forfaldne udstedte fakturaer. |
| `invoice_interest_calc` | `invoice interest` | `{ company, documentId? | invoiceNumber?, asOf, referenceRate }` | `{ interestAmount, baseAmount, days, ratePct }` | Beregner morarente (uden at registrere). |
| `invoice_compensation_calc` | `invoice compensation` | `{ company, documentId? | invoiceNumber?, asOf, amountDkk? }` | `{ compensationAmount, baseAmount }` | Beregner kompensationskrav for sen betaling. |
| `invoice_validate` | `invoice validate` | `{ payload: InvoicePayload }` | `{ ok, errors[], appliedRules[] }` | Validerer faktura-payload uden at gemme. |
| `journal_list` | `journal list` | `{ company }` | `{ entries: JournalEntryRow[] }` | Lister finansposteringer. |
| `period_list` | (afledt af `period close` + `accounts list`)¹ | `{ company }` | `{ periods: AccountingPeriodRow[] }` | Lister regnskabsperioder. Kræver ny CLI-kommando (se Forudsætninger). |
| `reconcile_bank` | `reconcile bank` | `{ company, from, to, status?, textMatch?, amount? }` | `{ matched: [...], unmatched: [...], totals }` | Bygger bank-afstemningsrapport for periode. |
| `retention_status` | `retention status` | `{ company, asOf? }` | `{ rows: RetentionStatusRow[], expired, dueWithin30d }` | Viser opbevaringsfrister og udløbet materiale. |
| `system_backup_status` | `system backup-status` | `{ company, asOf? }` | `{ compliant, lastBackupAt, dueAt, hoursOverdue? }` | Tjekker om backup-pligten er opfyldt. |
| `system_healthcheck` | `system healthcheck` | `{ company }` | `{ ok, missing[] }` | Tjekker virksomhedsmappens integritet. |
| `vat_report` | `vat report` | `{ company, from, to }` | `{ outputVat, inputVat, reverseCharge, netPayable, lines }` | Bygger momsrapport for perioden. |
| `vendor_list` | `vendor list` | `{ company, archived? }` | `{ vendors: VendorRecord[] }` | Lister kendte leverandører. |

¹ `period_list` kræver en ny CLI-kommando der wrapper en SELECT mod
`accounting_periods`-tabellen. Dokumenteret i Forudsætninger.

## Write-tools

Alle write-tools kræver `confirm: true`. Hvis flaget mangler returneres
`{ ok: false, errors: ["confirm: true required for write tool <name>"] }`
uden at kalde kernen.

### write-reversible

| Tool | CLI-ækvivalent | Input | Output | Brief |
|---|---|---|---|---|
| `bank_import` | `bank import` | `{ company, csvContent | csvPath, confirm }` | `BankImportResult` | Importerer banktransaktioner. Kan slettes ved at importere en ny CSV (vi har ikke implementeret slet, men import er deterministisk via `sourceFileHash`). |
| `customer_create` | `customer create` | `{ company, input: CreateCustomerInput, confirm }` | `{ customer: CustomerRecord }` | Opretter append-only kundepost. Kan arkiveres (ikke slettes). |
| `documents_ingest` | `documents ingest` | `{ company, filePath, metadata: DocumentMetadata, vendorId?, force?, confirm }` | `IngestDocumentResult` | Indlæser og hash-lagrer et bilag. Kan superseedes af nyt bilag. |
| `exception_resolve` | `exceptions resolve` | `{ company, id, note?, confirm }` | `{ exception: ExceptionRow }` | Markerer exception som løst. Kan ikke gen-åbnes manuelt. |
| `vendor_create` | `vendor create` | `{ company, input: CreateVendorInput, confirm }` | `{ vendor: VendorRecord }` | Opretter append-only leverandørpost. |

### write-irreversible

| Tool | CLI-ækvivalent | Input | Output | Brief |
|---|---|---|---|---|
| `company_init` | `init` | `{ company, cvr?, fiscalYearStartMonth?, fiscalYearLabelStrategy?, confirm }` | `{ company, accountsSeeded, ... }` | Initialiserer virksomhedsmappe + standardkontoplan. |
| `expense_book` | `expense book` | `{ company, documentId, bankTransactionId, expenseAccount, vatTreatment?, paymentAccount?, date?, text?, confirm }` | `BookExpenseFromBankResult` | Bogfører leverandørudgift fra bilag + bankpost. |
| `invoice_apply_payment` | `invoice apply-payment` | `{ company, payload: InvoicePaymentPayload, confirm }` | `{ paymentId, openBalance, status }` | Registrerer fakturabetaling fra payload. |
| `invoice_claim_compensation` | `invoice claim-compensation` | `{ company, documentId? | invoiceNumber?, asOf, amountDkk?, note?, confirm }` | `{ claimId }` | Registrerer kompensationskrav (uden at bogføre). |
| `invoice_claim_interest` | `invoice claim-interest` | `{ company, documentId? | invoiceNumber?, asOf, referenceRate, note?, confirm }` | `{ claimId, interestAmount }` | Registrerer morarentekrav. |
| `invoice_credit_note` | `invoice credit-note` | `{ company, payload: CreditNotePayload, confirm }` | `{ creditNoteId, creditNoteNo, ledgerEntryId }` | Udsteder kreditnota mod eksisterende faktura. |
| `invoice_issue` | `invoice issue` | `{ company, payload: InvoicePayload, customerId?, confirm }` | `{ documentId, invoiceNo, pdfPath, sha256 }` | Udsteder kundefaktura + immutable snapshot. |
| `invoice_post` | `invoice post` | `{ company, documentId? | invoiceNumber?, confirm }` | `JournalPostResult` | Bogfører udstedt faktura i finansen. |
| `invoice_post_compensation` | `invoice post-compensation` | `{ company, documentId? | invoiceNumber?, date?, confirm }` | `JournalPostResult` | Bogfører registreret kompensation. |
| `invoice_post_interest` | `invoice post-interest` | `{ company, documentId? | invoiceNumber?, claimId?, date?, confirm }` | `JournalPostResult` | Bogfører registreret morarentekrav. |
| `invoice_post_reminder` | `invoice post-reminder` | `{ company, documentId? | invoiceNumber?, reminderId?, date?, confirm }` | `JournalPostResult` | Bogfører registreret rykker. |
| `invoice_refund_bank` | `invoice refund-bank` | `{ company, payload: RefundPayload, confirm }` | `JournalPostResult` | Bogfører refundering til kunde fra banken. |
| `invoice_remind` | `invoice remind` | `{ company, documentId? | invoiceNumber?, date, fee?, note?, confirm }` | `{ reminderId, fee }` | Registrerer rykker på forfalden faktura. |
| `invoice_render` | `invoice render` | `{ company, documentId? | invoiceNumber?, confirm }` | `{ pdfPath, sha256, regenerated }` | Renderer (eller genskaber) deterministisk PDF for udstedt faktura. |
| `invoice_settle_bank` | `invoice settle-bank` | `{ company, payload: SettlementPayload, confirm }` | `JournalPostResult` | Matcher bankbetaling mod faktura. |
| `invoice_settle_claim_bank` | `invoice settle-claim-bank` | `{ company, payload: ClaimSettlementPayload, confirm }` | `JournalPostResult` | Matcher bankbetaling mod fakturakrav. |
| `invoice_write_off_bad_debt` | `invoice write-off-bad-debt` | `{ company, payload: BadDebtPayload, confirm }` | `JournalPostResult` | Bogfører tab på debitor. |
| `journal_post` | `journal post` | `{ company, payload: JournalEntryInput, confirm }` | `JournalPostResult` | Bogfører manuel finanspostering. |
| `journal_reverse` | `journal reverse` | `{ company, entryId? | entryNo? | matchText?, matchDate?, matchDocumentId?, date, reason, confirm }` | `JournalReverseResult` | Tilbagefører bogført finanspostering ved at oprette modpost. |
| `period_close` | `period close` | `{ company, from, to, kind?, status?, reference?, confirm }` | `CloseAccountingPeriodResult` | Lukker eller markerer regnskabsperiode. |
| `vat_post_eu_service_purchase` | `vat post-eu-service-purchase` | `{ company, payload: ReverseChargePurchaseInput, confirm }` | `JournalPostResult` | Bogfører EU-servicekøb med reverse charge. |
| `vat_post_representation_purchase` | `vat post-representation-purchase` | `{ company, payload: RepresentationPurchaseInput, confirm }` | `JournalPostResult` | Bogfører repræsentationsudgift med delvis momsfradrag. |

## System-tools

| Tool | CLI-ækvivalent | Klassifikation | Input | Output | Brief |
|---|---|---|---|---|---|
| `system_backup` | `system backup` | write-irreversible | `{ company, at?, confirm }` | `CreateSystemBackupResult` | Opretter revisionsklar backup. |
| `system_export_authority` | `system export-authority` | write-irreversible | `{ company, from, to, out, requestedAt?, requester?, confirm }` | `ExportAuthorityPackageResult` | Eksporterer materiale til myndighedsudlevering. |
| `system_restore_backup` | `system restore-backup` | **destructive** | `{ backupDir, targetCompany, verifyKey?, confirm, confirmText }` | `RestoreSystemBackupResult` | Gendanner backup til ny virksomhedssti. `confirmText` skal være `"RESTORE <targetCompany>"`. |

## Eksempel-handshakes

### Read-tool: `audit_verify`

Input (MCP `tools/call`):
```json
{
  "name": "audit_verify",
  "arguments": {
    "company": "/Users/mikkel/companies/acme-aps"
  }
}
```

Output:
```json
{
  "ok": true,
  "data": {
    "entries": 142,
    "ok": true,
    "errors": []
  },
  "appliedRules": ["DK-BOOKKEEPING-AUDIT-CHAIN-001"]
}
```

### Read-tool: `invoice_status`

Input:
```json
{
  "name": "invoice_status",
  "arguments": {
    "company": "/Users/mikkel/companies/acme-aps",
    "invoiceNumber": "2026-00042",
    "asOf": "2026-05-18"
  }
}
```

Output:
```json
{
  "ok": true,
  "data": {
    "documentId": 87,
    "invoiceNo": "2026-00042",
    "grossAmount": 12500.00,
    "paidAmount": 5000.00,
    "openBalance": 7500.00,
    "status": "partial",
    "dueDate": "2026-04-30",
    "daysOverdue": 18
  }
}
```

### Write-tool: `journal_post`

Input:
```json
{
  "name": "journal_post",
  "arguments": {
    "company": "/Users/mikkel/companies/acme-aps",
    "payload": {
      "transactionDate": "2026-05-18",
      "text": "Manuel postering — kontorartikler",
      "documentId": 12,
      "lines": [
        { "accountNo": "3617", "debitAmount": 320.00, "vatCode": "I25" },
        { "accountNo": "6902", "debitAmount": 80.00 },
        { "accountNo": "5820", "creditAmount": 400.00 }
      ]
    },
    "confirm": true,
    "idempotencyKey": "8c2a6b1e-1d4f-4b9b-9c43-aa5e0f6f3d11"
  }
}
```

Output:
```json
{
  "ok": true,
  "data": {
    "entryId": 142,
    "entryNo": "2026-00142",
    "entryHash": "f4a1...e0b9"
  },
  "appliedRules": [
    "DK-BOOKKEEPING-BALANCED-001",
    "DK-BOOKKEEPING-DOCUMENT-001",
    "DK-BOOKKEEPING-PERIOD-LOCK-001"
  ]
}
```

### Destructive-tool: `system_restore_backup`

Input:
```json
{
  "name": "system_restore_backup",
  "arguments": {
    "backupDir": "/Users/mikkel/backups/acme-aps/2026-05-17T22-00-00Z",
    "targetCompany": "/Users/mikkel/companies/acme-aps-restored",
    "verifyKey": "/Users/mikkel/keys/acme-aps-backup.pub",
    "confirm": true,
    "confirmText": "RESTORE /Users/mikkel/companies/acme-aps-restored"
  }
}
```

Output (success):
```json
{
  "ok": true,
  "data": {
    "restoredCompany": "/Users/mikkel/companies/acme-aps-restored",
    "filesRestored": 1842,
    "manifestVerified": true,
    "signatureVerified": true
  }
}
```

### Fejl-respons (universel)

Manglende `confirm` på write-tool:
```json
{
  "ok": false,
  "errors": ["confirm: true required for write tool journal_post"]
}
```

Validation-fejl fra kernen:
```json
{
  "ok": false,
  "errors": [
    "Postering går ikke i nul: debit 320.00, credit 400.00 (diff 80.00)"
  ],
  "appliedRules": ["DK-BOOKKEEPING-BALANCED-001"]
}
```

Forkert `confirmText` på destructive-tool:
```json
{
  "ok": false,
  "errors": [
    "confirmText must match 'RESTORE <targetCompany>' exactly (got: 'restore acme')"
  ]
}
```

## Actor-attribution

MCP-serveren sætter `RENTEMESTER_ACTOR` env-var per kald (ikke globalt — det
serialiseres ind i kerne-funktionens `resolveActor()`-input via
`createdBy`/`createdByProgram`).

Mapping fra MCP-client-info til actor-streng:

| Felt | Kilde | Eksempel |
|---|---|---|
| `createdBy` | MCP-client-id + bruger-context | `agent:claude-code` |
| `createdByProgram` | MCP-client `name/version` | `claude-code/0.4.1` |
| `auditActor` | Beregnet `"<createdBy> via <createdByProgram>"` | `agent:claude-code via claude-code/0.4.1` |

Hvis MCP-klienten sender en `userContext` (fx via custom header eller MCP
`clientInfo.userId`), opgraderes `createdBy` til
`agent:<client>:<userId>` — fx `agent:claude-code:mikkel@56n.dk`. Dette
matcher actor-allowlist-arbejdet i #63.

Hver write-tool tilskriver derudover automatisk:
- `audit_log.event_type` = tool-navn (`journal_post`, `invoice_issue`, …)
- `audit_log.actor` = `auditActor`
- `audit_log.entity_type` + `entity_id` = den primære nyligt oprettede entitet.

## Forudsætninger

Følgende skal være på plads før implementation (#78):

1. **Dependencies tilføjes til `package.json`**:
   - `@modelcontextprotocol/sdk` — MCP-server runtime.
   - `zod` — input-validering. (Ikke i nuværende dependency-træ.)

2. **Ny CLI-kommando `period list`** (lille tilføjelse i `cli-meta.ts` +
   `cli.ts` + en SELECT i `core/periods.ts`). MCP-tool `period_list`
   afhænger af denne. Alternativ: eksponer kun via MCP og hold CLI uden
   `period list`, men det bryder princippet "1 MCP-tool = 1 CLI-kommando".

3. **Ny CLI-kommando `audit log` (read)** der lister `audit_log`-tabellen.
   Ikke i den oprindelige issue-scope men nødvendig hvis agenter skal kunne
   debugge egne kald uden direkte DB-adgang. Markeres som stretch-goal.

4. **`actor`-flag normalisering**: CLI accepterer allerede `--actor` og
   `--actor-via` (jf. `cli-meta.ts` GLOBAL_FLAGS). MCP-serveren skal kalde
   kernens `resolveActor({ createdBy, createdByProgram })` direkte i stedet
   for at gå gennem env-var — env-var virker, men er race-condition-prone
   ved parallelle MCP-kald i samme proces. Anbefalet ændring: alle
   write-kernel-funktioner accepterer en `actor?: ResolveActorInput`-parameter.
   (Nogle gør det allerede via `createdBy`/`createdByProgram` i payload —
   tjek og udvid hvor manglende.)

5. **Idempotency-cache**: en SQLite-tabel (eller in-memory map) der mapper
   `idempotencyKey` → serialiseret response i 24h. Ny migration eller en
   sidecar-fil i company-mappen.

6. **Strukturerede output-typer**: kernen returnerer i dag `*Result`-typer
   med `ok`/`errors`/`appliedRules` direkte på top-level. MCP-tools wrapper
   disse i `{ ok, data, errors, appliedRules }` så `data` indeholder
   selve nyttelasten uden `ok`/`errors`-felterne. En lille adapter-funktion
   i MCP-laget tager sig af dette.

7. **Tool-registret som single-source-of-truth**: `src/mcp/tools.ts` (når
   skrevet i #77) eksporterer en array af `McpToolSpec`-objekter. En unit-test
   sammenligner registret med `COMMAND_SPECS` og fejler hvis en CLI-kommando
   mangler tool-mapping (eller omvendt).

8. **`confirmText`-håndhævelse på destructive tools** kræver en ny helper
   `requireConfirmText(input, expected)` i MCP-laget. Den findes ikke i
   kernen i dag (CLI har ingen destructive-tools — `system restore-backup`
   kan i dag køres uden bekræftelse fra CLI).

## Tool-count summary

- **Read-tools**: 22
- **Write-reversible**: 5
- **Write-irreversible**: 22 (inkl. `company_init`, `system_backup`,
  `system_export_authority`)
- **Destructive**: 1 (`system_restore_backup`)
- **Total**: 50

Bemærk: nogle CLI-kommandoer er bevidst slået sammen til samme tool (fx
`invoice render` og `invoice render --regenerate` håndteres af samme
`invoice_render`-tool), og enkelte beregningskommandoer er klassificeret som
`read` selvom CLI-navnet kunne forvirre (`invoice interest` beregner kun;
`invoice claim-interest` registrerer).
