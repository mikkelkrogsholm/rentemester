# MCP Tool Surface — Rentemester

Den autoritative liste over de tools Rentemester-MCP-serveren eksponerer til
agenter (Claude, Cursor, Claude Code, Codex osv.). Dokumentet startede som
bygge-tegning for MCP-epicen (#89, scaffold #77, implementation #78) og
vedligeholdes nu som facitliste mod den kørende server.

> **Hold dette synkront.** Tool-tallet i dette dokument skal matche en
> kørende server. Den hurtige måde at få den faktiske liste på er at drive
> serveren over stdio og kalde `tools/list` — se `scripts/smoke-mcp.ts` for
> et minimalt eksempel. Tæl aldrig tools i hånden.

Kilder:
- En kørende `src/mcp/server.ts` (`tools/list`) — facit for hvilke tools der
  faktisk eksponeres og deres `annotations` (read-only/destructive-hints).
- `src/mcp/registry.ts` — registrerer hele tool-surface'en pr. domæne.
- `src/cli-meta.ts` — CLI-kommandoerne. MCP-surface'en er *tæt på* 1:1 med
  CLI'en, men ikke fuldstændig — se "CLI/MCP-mapping" nedenfor.
- `src/core/*.ts` — TypeScript-typer for inputs og resultater (`InvoicePayload`,
  `JournalEntryInput`, `BankImportRow`, `DocumentMetadata`, `ActorContext` osv.).
- `src/cli-format.ts` — output-konventionen `{ ok, errors, ... }` som vi
  genbruger til MCP-svar.

For den narrative kontrakt — hvordan en ekstern agent skal bruge den løse
tool-surface (rækkefølge, confirm/destructive-konventioner, hvor
forudsætninger ligger) — se [`docs/mcp-agent-contract.md`](mcp-agent-contract.md).
Serveren leverer derudover en kort `instructions`-streng i `initialize`-svaret
med det samme i komprimeret form.

## Designprincipper

1. **En MCP-tool = (typisk) én CLI-kommando.** Tools navngives `snake_case`
   med første led som domæne (`invoice_*`, `bank_*`, `journal_*`,
   `documents_*`, `system_*`, `vat_*`, `customer_*`, `vendor_*`, `period_*`,
   `retention_*`, `exceptions_*`, `accounts_*`, `reconcile_*`, `expense_*`,
   `audit_*`, `asset_*`, `mileage_*`, `recurring_invoice_*`, `mail_intake_*`,
   `imap_intake_*`, `peppol_*`, `company_*`, `portfolio_*`, `import_*`).
   Dette matcher CLI'ens `domæne underkommando`-struktur tæt — men ikke
   100 %: nogle MCP-tools har ingen CLI-pendant og enkelte CLI-kommandoer
   eksponeres ikke som tools. Kendte afvigelser er listet under
   "CLI/MCP-mapping".
2. **Typed inputs via zod genereret fra TypeScript-typerne.** For hver tool
   defineres en `z.object({...})`. Hvor kernen allerede har en type
   (`InvoicePayload`, `JournalEntryInput`, `BankImportRow`,
   `DocumentMetadata`, `CreateCustomerInput`, `CreateVendorInput`,
   `BookExpenseFromBankInput`, `CloseAccountingPeriodInput`,
   `ReverseChargePurchaseInput`, `RepresentationPurchaseInput`,
   `ExportAuthorityPackageInput`, `RestoreSystemBackupInput`,
   `RecordExceptionInput`, `ResolveExceptionInput`) genereres zod-skemaet
   parallelt.
3. **Struktureret output `{ ok, data?, errors[], appliedRules? }`.** Vi
   genbruger kernens eksisterende `JournalPostResult`/`*Result`-shape og
   wrapper alle outputs i et fælles convolut. `ok=true` ⇒ `data` er sat;
   `ok=false` ⇒ `errors` er en ikke-tom string-liste. `appliedRules` listes
   altid for kommandoer der bogfører (sporbarhed mod regelsæt).
4. **Sikkerhedsklassifikation** på fire niveauer:
   - `read` — ingen state-bivirkninger; agenten må kalde frit og parallelt.
     Markeret med `annotations.readOnlyHint: true`.
   - `write-reversible` — opretter state der kan tilbageføres via
     `journal_reverse`, `invoice_credit_note`, `exception_resolve` eller ved
     en korrigerende post. Kræver `confirm: true`.
   - `write-irreversible` — bogfører i append-only kæde (audit_log + hash);
     kan kun "rulles tilbage" via en modpostering. Kræver `confirm: true`.
   - `destructive` — system-niveau (restore). Markeret med
     `annotations.destructiveHint: true`. Kræver `confirm: true` **og**
     `confirmText: "<præcis fritekst>"`.
5. **Actor-attribution er obligatorisk.** Hvert MCP-call tilskrives som
   `agent:<client-info>` (jf. #63). `auditActor` skrives ind i
   `audit_log.actor` og udgør traceable kæde fra agent-call til bogføring.
6. **Ingen generel idempotency-key på writes.** Der findes *ikke* en
   `idempotencyKey`-mekanisme med en retry-cache på writes. En agent kan
   derfor ikke regne med at en gentaget `journal_post` (eller anden write)
   automatisk de-dupes — en retry efter en uafklaret netværksfejl kan
   dobbelt-bogføre. Flere intake-/generér-tools er derimod idempotente *af
   natur* (`annotations.idempotentHint`) — de de-duper på indhold/periode,
   ikke på en klient-leveret nøgle; se de enkelte tool-rækker nedenfor.
   En generel write-idempotency-cache er en mulig fremtidig udvidelse, ikke
   et nuværende løfte.
7. **Eksplicit `company`-parameter overalt.** Aldrig implicit "current
   company"; agent skal altid pege på virksomheden. `company` accepterer
   **enten** en absolut filsystem-sti til virksomhedsmappen (`..`-guardet),
   **eller** en workspace-slug — et bart, separator-frit slug-token der slås
   op i manifestet for det workspace `RENTEMESTER_WORKSPACE` peger på.
   En værdi med `/` eller `\` behandles altid som en sti, så en rigtig sti
   aldrig fejltolkes som slug (`resolveCompanyArg` i
   `src/mcp/tool-runtime.ts`). Workspace-tools (`company_add`,
   `portfolio_overview`) tager i stedet en `workspace`-sti.

## Klassifikation

| Niveau | Krav | Eksempler |
|---|---|---|
| `read` | Ingen | `audit_verify`, `bank_list`, `invoice_status`, `vat_report`, `portfolio_overview` |
| `write-reversible` | `confirm: true` | `customer_create`, `vendor_create`, `bank_import`, `documents_ingest`, `exception_resolve`, `mileage_log` |
| `write-irreversible` | `confirm: true` | `journal_post`, `invoice_issue`, `invoice_post`, `expense_book`, `vat_post_*`, `asset_register`, `system_backup` |
| `destructive` | `confirm: true` + `confirmText` | `system_restore_backup` |

`journal_reverse` er klassificeret som `write-irreversible`: den skriver en ny
post i den append-only kæde — den modposterer en tidligere post, men kæden
selv ændres ikke.

## Resultat-shapes (`outputSchema`)

**Alle 95 tools deklarerer et `outputSchema`** (#202). Det er det samme
delte schema for hver tool — konvolutten — så en agent kan læse
resultat-kontrakten fra `tools/list` *uden* at kalde tool'et først.
Schemaet er defineret én gang i `src/mcp/envelope.ts` (`envelopeShape`).

Konvolutten (`structuredContent` på et `tools/call`-svar):

| Felt | Type | Hvornår |
|---|---|---|
| `ok` | `boolean` | Altid. `true` ⇒ kaldet lykkedes; `false` ⇒ se `errors`. |
| `data` | `object` | Kun ved `ok:true`. Kerne-resultatet. Udeladt ved `ok:false`. |
| `errors` | `string[]` | Altid. Tom ved `ok:true`; ikke-tom ved `ok:false`. |
| `appliedRules` | `string[]` | Valgfri. Regel-id'er der fyrede (sættes for bogførings-tools). |

`outputSchema` typer bevidst `data` som et **åbent objekt** (`passthrough`):
den konkrete feltliste i `data` varierer pr. tool, og MCP-SDK'en validerer
kun `structuredContent` mod schemaet for *succes*-svar (`isError:false`) —
fejl-envelopes springes over. De per-tool `data`-felter er ikke hånd-typet
95 gange; de er dokumenteret nedenfor og i tool-brief'ene.

### `data`-felter pr. tool — det der har betydning

`read`-tools returnerer typisk en liste plus en tæller:

| Tool(s) | `data`-felter |
|---|---|
| `accounts_list` | `{ accounts: [{ accountNo, name, type, defaultVatCode }], count }` |
| `journal_list` | `{ entries: [{ id, entryNo, transactionDate, text, currency, amountForeign, amountDkk, fxRateToDkk, documentId, sourceBankTransactionId, status, reversalOfEntryId }], count }` |
| `journal_dry_run` | `{ entryId, entryNo, previousHash, entryHash, accountEffects: [{ accountNo, accountName, balanceBefore, balanceAfter, delta }] }` — ikke-bindende forhåndsvisning af `journal_post`: felterne beskriver hvad posteringen *ville* få. `accountEffects` lister saldo før/efter pr. berørt konto (debet-minus-kredit-netto, i kroner). Ved en ugyldig payload er konvolutten `ok=false` med `errors[]`, og `data` mangler. |
| `bank_list` | `{ rows: [...], count }` |
| `invoice_list` | `{ invoices: [...], count }` |
| `exceptions_list` | `{ exceptions: [...], count }` |
| `period_list` | `{ periods: [{ id, periodStart, periodEnd, kind, status, reference, createdAt }], count }` — `kind` er `"vat_quarter" \| "fiscal_year" \| "custom"`; `status` er `"open" \| "closed" \| "reported"`; `reference` kan være `null`. |
| `audit_verify` | `{ entries: <number> }` — kun antallet af verificerede posteringer. Integritets-verdikten læses fra **konvolutten**: `ok=true` (+ tom `errors[]`) ⇒ kæden er intakt; `ok=false` ⇒ `errors[]` lister bruddene. Der er hverken `ok` eller `errors` *inde i* `data`. |
| `invoice_status` | `{ invoiceDocumentId, invoiceNumber, grossAmount, creditedAmount, paidAmount, openBalance, claimOpenBalance, asOfDate, dueDate, effectiveDueDate, isOverdue, overdueDays, status, payments[], creditNotes[], refunds[], claimPayments[], badDebtWriteOffs[], reminders[], compensationClaims[], interestClaims[], totalReminderFees, totalCompensationClaims, totalInterestClaims, totalClaimPayments, totalBadDebtWrittenOff }` — feltet hedder `invoiceDocumentId` (ikke `documentId`), `invoiceNumber` (ikke `invoiceNo`) og `overdueDays` (ikke `daysOverdue`). `status` er `"open" \| "paid" \| "credited" \| "refunded" \| "overpaid" \| "written_off"`. Den fulde typedefinition er `InvoiceStatusResult` i `src/core/invoice-payments.ts`. |

`write`-tools returnerer id'er + hashes på den nyligt oprettede entitet:

| Tool | `data`-felter |
|---|---|
| `journal_post` | `{ entryId, entryNo, entryHash }` |
| `invoice_issue` | `{ documentId, invoiceNumber, storedPath, sha256, pdfDocumentId?, pdfStoredPath?, pdfSha256? }` — feltet hedder `documentId` (ikke `invoiceDocumentId`); `invoiceNumber` (ikke `invoiceNo`). |
| `customer_create` / `vendor_create` | `{ customerId }` / `{ vendorId }` |
| `journal_reverse` | `{ entryId, entryNo, entryHash }` for modposten |
| `recurring_invoice_create` | `{ templateId }` |
| `recurring_invoice_generate` | `{ created, templateId, periodIndex, documentId, invoiceNumber, issueDate, dueDate, deliveryPeriodStart?, deliveryPeriodEnd? }` — `created:false` ⇒ en eksisterende faktura blev returneret (idempotent). |
| `asset_register` | `{ assetId, totalPeriods, periodAmount }` |
| `mileage_log` | `{ mileageEntryId, entryNo, amountBasis }` |
| `period_close` | `{ periodId, periodStart, periodEnd, kind, status, reference? }` |
| `invoice_send_email` | `{ invoiceNumber, kind, recipient, subject, messageId, duplicate }` — `duplicate:true` ⇒ en identisk afsendelse fandtes allerede (idempotent). |
| `customer_validate_vat` | `{ validation: { … VIES-record … } }` |
| `audit_verify` | `{ entries }` — kun antallet af verificerede posteringer. Integritets-verdikten er **konvoluttens** `ok`/`errors[]`, ikke et felt i `data`: `ok=true` ⇒ kæden er intakt, `ok=false` ⇒ `errors[]` lister bruddene. |
| `system_restore_backup` | `{ backupId, restoredAt, targetCompanyRoot, restoredDbPath, restoredFiles: { documentsOriginals, invoicesIssued, config } }` — `backupId`/`restoredAt` er ISO-tidsstempler; `restoredFiles`-felterne er antal genskabte filer pr. kategori. `appliedRules` (på konvoluttens topniveau) er `["DK-BOOKKEEPING-RESTORE-001"]`. |

> **Discovery-kontrakten:** Konvolut-formen er maskin-kendt via `outputSchema`
> i `tools/list`. Den præcise `data`-feltliste står her og i kildens
> `*Result`-typer (`src/core/*.ts`, fx `IssueInvoiceResult`,
> `JournalEntryResult`). En agent behøver derfor ikke kalde et tool blot for
> at lære dets resultat-shape at kende.

## Tool-count summary

Tallene gælder en kørende `src/mcp/server.ts` (verificeret via `tools/list`).

- **Read-tools**: 42
- **Write-reversible**: 11
- **Write-irreversible**: 41
- **Destructive**: 1 (`system_restore_backup`)
- **Total**: **95**

## Read-tools

42 tools. Ingen state-bivirkninger; må kaldes frit og parallelt.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `accounts_list` | `accounts list` | `{ company }` | Lister kontoplanen. |
| `accrual_register_report` | `accrual register-report` | `{ company }` | Register af periodeafgrænsningsposter med bogførte perioder, periodiseret beløb og resterende balanceeksponering. |
| `asset_register_report` | `asset register-report` | `{ company }` | Aktivregister med akkumulerede afskrivninger og bogført værdi. |
| `audit_verify` | `audit verify` | `{ company }` | Verificerer hash-chain og bogføringsintegritet. |
| `bank_list` | `bank list` | `{ company, status?, from?, to?, textMatch?, amount?, account? }` | Lister importerede banktransaktioner med filtre. |
| `bank_suggest_matches` | `bank suggest-matches` | `{ company, bankTransactionId?, max? }` | Foreslår deterministiske match mellem uafstemte bank-poster og bilag. |
| `budget_forecast` | `budget forecast` | `{ company, startDate, months }` | Likviditetsprognose: fremskriver banksaldoen måned for måned ud fra primosaldo, åbne fakturaer der forfalder, planlagte gentagne fakturaer og budgetterede omkostninger. Rent deterministisk. |
| `budget_list` | `budget list` | `{ company, period?, accountNo? }` | Lister de gældende (seneste-revision) budgetlinjer. |
| `budget_vs_actual` | `budget vs-actual` | `{ company, from, to }` | Sammenligner budget mod faktisk bogføring pr. konto pr. måned. |
| `customer_list` | `customer list` | `{ company, archived? }` | Lister kendte kunder. |
| `customer_validate_vat` | `customer validate-vat` | `{ company, cvr }` | Validerer EU-VAT via VIES og opdaterer en lokal validerings-cache. Klassificeret `read` (se note nedenfor): den skriver kun en gennemsigtig opslags-cache, ingen bogførings-/stamdata-state, og kræver ikke `confirm`. |
| `cvr_lookup` | `customer cvr-lookup` | `{ company, cvr }` | Slår en dansk virksomhed op i CVR-registret. Kræver `CVR_USERNAME`/`CVR_PASSWORD`. |
| `documents_list` | `documents list` | `{ company }` | Lister gemte bilag. |
| `exceptions_list` | `exceptions list` | `{ company, status?, includeArchived? }` | Lister exceptions-køen (open/resolved/all). |
| `import_archive_list` | `import archive` | `{ company, sourceSystem? }` | Lister pre-cut-over regnskabsår arkiveret fra et flerårigt eksport. |
| `import_archive_year` | (afledt af `import archive`)¹ | `{ company, fiscalYear, sourceSystem? }` | Henter ét arkiveret regnskabsårs fulde posteringer + saldobalance. |
| `invoice_compensation_calc` | `invoice compensation` | `{ company, documentId? \| invoiceNumber?, asOf, amountDkk? }` | Beregner kompensationskrav (uden at registrere). |
| `invoice_find` | `invoice find` | `{ company, query?, customer?, invoiceNumber?, amount?, asOf? }` | Søger fakturaer på nummer, kunde eller beløb. |
| `invoice_interest_calc` | `invoice interest` | `{ company, documentId? \| invoiceNumber?, asOf, referenceRate }` | Beregner morarente (uden at registrere). |
| `invoice_list` | `invoice list` | `{ company, status?, from?, to?, customerCvr?, customer?, invoiceNumber?, minAmount?, maxAmount?, asOf? }` | Lister udstedte fakturaer med filtre. |
| `invoice_overdue` | `invoice overdue` | `{ company, asOf?, minDays? }` | Lister forfaldne, ikke fuldt afregnede fakturaer. |
| `invoice_status` | `invoice status` | `{ company, documentId? \| invoiceNumber?, asOf? }` | Viser åben saldo og status på en faktura. |
| `invoice_validate` | `invoice validate` | `{ payload: InvoicePayload }` | Validerer faktura-payload uden at gemme. |
| `journal_dry_run` | `journal dry-run` | `{ company, payload: JournalEntryInput }` | Forhåndsviser hvad `journal_post` ville gøre — uden at skrive. Intet journalnummer forbruges. |
| `journal_list` | `journal list` | `{ company }` | Lister finansposteringer. |
| `mileage_list` | `mileage list` | `{ company }` | Lister registrerede kørselsposter. |
| `mileage_report` | `mileage report` | `{ company, from, to }` | Deterministisk periode-rapport over kilometer og beløbsgrundlag. |
| `payable_list` | `payable list` | `{ company, status?, asOfDate?, supplier?, vendorId?, from?, to?, minDays? }` | Bygger kreditorlisten: åbne leverandørposter med åben saldo og forfaldsintervaller (forfaldne/ikke-forfaldne). |
| `period_list` | (ingen — kun MCP)² | `{ company }` | Lister regnskabsperioder (open/closed/reported). |
| `portfolio_overview` | `dashboard` (delvist) | `{ workspace, asOf? }` | Status side om side for hver virksomhed i workspace'et. Intet konsolideres. |
| `reconcile_bank` | `reconcile bank` | `{ company, from, to, status?, textMatch?, amount?, account? }` | Bygger bank-afstemningsrapport for periode. |
| `recurring_invoice_list` | `recurring-invoice list` | `{ company, includeInactive? }` | Lister gentagende fakturaskabeloner. |
| `retention_status` | `retention status` | `{ company, asOf? }` | Viser opbevaringsfrister og udløbet materiale. |
| `system_backup_destination_list` | `system backup-destinations` | `{ company }` | Lister konfigurerede backup-destinationer med attestering. |
| `system_backup_governance` | `system backup-governance` | `{ company, asOf? }` | Samlet backup-status: forfald, lås, destinationer, sikker placering. |
| `system_backup_status` | `system backup-status` | `{ company, asOf? }` | Tjekker om backup-pligten er opfyldt. |
| `system_healthcheck` | `system healthcheck` | `{ company }` | Tjekker virksomhedsmappens integritet. |
| `tax_return_prepare` | `report tax` | `{ company, from, to }` | Forbereder selskabets skattepligtige indkomst (oplysningsskema) for et lukket regnskabsår: årets resultat + deterministiske skattemæssige reguleringer + 22% selskabsskat (kun ApS). Ikke-deterministiske poster markeres som needs-review. |
| `vat_eu_sales_list` | `vat eu-sales-list` | `{ company, from, to }` | EU-salg uden moms-liste (VIES recapitulative statement): værdien af grænseoverskridende B2B-salg uden dansk moms grupperet pr. køber-VAT-nummer. |
| `vat_oss_report` | `vat oss-report` | `{ company, from, to }` | OSS-rapportskelet (One Stop Shop, første slice): grundlaget for digitale ydelser solgt til EU-forbrugere. Ikke en OSS-indberetning til SKAT. |
| `vat_report` | `vat report` | `{ company, from, to }` | Bygger momsrapport for perioden. |
| `vendor_list` | `vendor list` | `{ company, archived? }` | Lister kendte leverandører. |

¹ `import_archive_year` har ingen selvstændig CLI-kommando; den hentes fra
samme arkiv-artefakt som `import archive` skriver.
² `period_list` — se "CLI/MCP-mapping" nedenfor.

> **`customer_validate_vat` — read/write-klassifikation.** Tool'et slår et
> EU-VAT-nummer op mod VIES og *skriver* resultatet til en lokal cache-tabel
> (`vies_validations`). Det er bevidst klassificeret `read`
> (`readOnlyHint: true`) og kræver derfor *ikke* `confirm: true`: den eneste
> side-effekt er en gennemsigtig opslags-cache med TTL — der skrives hverken
> i finanskæden eller i stamdata, og et gentaget opslag inden for TTL
> genbruger blot cachen (`idempotentHint: true`).
>
> **Bemærk en bevidst CLI/MCP-divergens i governance-klasse.** Den ramme
> handling er den samme (et cache-opdaterende VIES-opslag), men de to
> overflader klassificerer den forskelligt:
>
> - **MCP-tool'et `customer_validate_vat`** er `read` — det er *ikke*
>   `confirm`-gatet og kræver ingen actor.
> - **CLI-kommandoen `customer validate-vat`** står derimod i
>   `MUTATING_COMMANDS` (`src/cli-actor.ts`): den er actor-gatet og afvises
>   uden en kendt actor med `actor required for mutations`.
>
> Det er altså *ikke* korrekt at kalde de to "konsistente" — de sidder i
> materielt forskellige governance-klasser (read vs. actor-gatet mutation).
> Divergensen er accepteret: CLI'en behandler enhver cache-skrivende handling
> som muterende for at få actor-attribution på opslaget, mens MCP-laget
> vægter at et opslag skal kunne kaldes frit. Vil man harmonisere, skal
> enten CLI-kommandoen ud af `MUTATING_COMMANDS`, eller MCP-tool'et
> omklassificeres til en `confirm`-gatet write.

## Write-tools

Alle write-tools kræver `confirm: true`. Mangler flaget returneres
`{ ok: false, errors: ["confirm: true required for write tool <name>"] }`
uden at kernen kaldes.

> **Bemærk — det destruktive `system_restore_backup` afviger:** dets
> manglende-`confirm`-fejl lyder `confirm: true required for destructive
> tool system_restore_backup` (ordet **destructive**, ikke **write**). En
> agent der streng-matcher `required for write tool` rammer derfor IKKE
> restore-tool'et — match på `confirm: true required for` for at fange
> begge.

### write-reversible

11 tools. Opretter state der kan tilbageføres/arkiveres uden at røre den
append-only finanskæde.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `bank_import` | `bank import` | `{ company, csvPath \| csvContent, account?, profile?, confirm }` | Importerer banktransaktioner fra CSV. Deterministisk via `sourceFileHash`. |
| `budget_set` | `budget set` | `{ company, accountNo, period, amount, notes?, confirm }` | Sætter et budget for én konto i én kalendermåned. Append-only revisioner — seneste vinder. |
| `company_sync_cvr` | `company sync-cvr` | `{ company, confirm }` | Henter virksomhedens stamdata fra CVR og opdaterer companies-rækken. Regnskabsåret røres ikke. |
| `customer_create` | `customer create` | `{ company, input: CreateCustomerInput, fromCvr?, confirm }` | Opretter append-only kundepost. Kan arkiveres. |
| `documents_ingest` | `documents ingest` | `{ company, filePath, metadata: DocumentMetadata, vendorId?, force?, confirm }` | Indlæser og hash-lagrer et bilag. |
| `exception_resolve` | `exceptions resolve` | `{ company, id, note?, confirm }` | Markerer exception som løst. |
| `imap_intake_poll` | `imap-intake poll` | `{ company, imapHost, imapPort?, imapUsername, imapMailbox?, sinceUid?, metadata?, metadataPerMessage?, force?, confirm }` | Poller en IMAP-postkasse og videresender vedhæftninger til bilags-pipelinen. Dedup-stabil. |
| `mail_intake_ingest` | `mail-intake ingest` | `{ company, source, metadata?, metadataPerMessage?, force?, confirm }` | Indlæser en `.eml`-fil/maildrop-mappe og videresender vedhæftninger. Idempotent. |
| `mileage_export` | `mileage export` | `{ company, from, to, outputDir, confirm }` | Skriver et deterministisk eksport-artefakt (JSON + CSV) over kørselsregnskabet. |
| `mileage_log` | `mileage log` | `{ company, input, confirm }` | Tilføjer en append-only kørselspost. Skattemæssig behandling er brugerens ansvar. |
| `vendor_create` | `vendor create` | `{ company, input: CreateVendorInput, fromCvr?, confirm }` | Opretter append-only leverandørpost. |

### write-irreversible

41 tools. Bogfører i den append-only hash-kæde eller skriver
revisionsklare/eksterne artefakter; kan kun "rulles tilbage" via en
modpostering.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `accrual_recognize` | `accrual recognize` | `{ company, accrualId, period, date?, settlementAccountNo?, confirm }` | Indtægts-/omkostningsfører én periode af en periodeafgrænsningspost. |
| `accrual_register` | `accrual register` | `{ company, accrualType, description, totalAmount, recognitionPeriods, firstRecognitionDate, resultAccountNo, registrationDate?, periodStepMonths?, balanceAccountNo?, settlementAccountNo?, documentId?, note?, confirm }` | Registrerer en periodeafgrænsningspost og bogfører registreringsposteringen. |
| `asset_depreciate` | `asset depreciate` | `{ company, assetId, period, date, confirm }` | Bogfører en periodes afskrivning. |
| `asset_register` | `asset register` | `{ company, name, category, acquisitionDate, cost, usefulLifeMonths, documentId, assetAccount, depreciationAccount, accumulatedAccount, note?, confirm }` | Registrerer et aktiv med lineær afskrivningsplan. |
| `asset_write_off` | `asset write-off` | `{ company, name, category, acquisitionDate, cost, documentId, expenseAccount, date, thresholdRuleSource, confirmImmediateWriteOff, paymentAccount?, note?, confirm }` | Bogfører straksafskrivning af et mindre aktiv. |
| `company_add` | `company add` | `{ workspace?, name, slug?, cvr?, fiscalYearStartMonth?, fiscalYearLabelStrategy?, confirm }` | Opretter en ny virksomhed under `<workspace>/<slug>/` og initialiserer ledgeren. Som ethvert write-tool kræver det `confirm: true` — uden flaget returneres `{ ok:false, errors:["confirm: true required for write tool company_add"] }` uden at noget oprettes. Udelades `workspace`, bruges miljøvariablen `RENTEMESTER_WORKSPACE` på MCP-serverens host; er den heller ikke sat, afvises kaldet med `no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE`. **Ikke idempotent (`idempotentHint: false`):** et gentaget kald med samme `name`/`slug` *afvises* — det overskriver ALDRIG en eksisterende virksomhed. Findes der allerede en ledger på `<workspace>/<slug>/` fejler kaldet med `a company already exists at <sti>`, og et slug der allerede står i workspace-manifestet afvises ligeledes med en `ok:false`-envelope. For at oprette endnu en virksomhed med samme navn skal et nyt, unikt `slug` angives eksplicit. |
| `expense_book` | `expense book` | `{ company, documentId, bankTransactionId, expenseAccount, vatTreatment?, paymentAccount?, date?, text?, confirm }` | Bogfører leverandørudgift fra bilag + bankpost. |
| `invoice_apply_payment` | `invoice apply-payment` | `{ company, payload: InvoicePaymentPayload, confirm }` | Registrerer fakturabetaling fra payload. |
| `invoice_claim_compensation` | `invoice claim-compensation` | `{ company, documentId? \| invoiceNumber?, asOf, amountDkk?, note?, confirm }` | Registrerer kompensationskrav. |
| `invoice_claim_interest` | `invoice claim-interest` | `{ company, documentId? \| invoiceNumber?, asOf, referenceRate, note?, confirm }` | Registrerer morarentekrav. |
| `invoice_credit_note` | `invoice credit-note` | `{ company, payload: CreditNotePayload, confirm }` | Udsteder kreditnota mod eksisterende faktura. |
| `invoice_issue` | `invoice issue` | `{ company, payload: InvoicePayload, customerId?, confirm }` | Udsteder kundefaktura + immutable snapshot. |
| `invoice_post` | `invoice post` | `{ company, documentId? \| invoiceNumber?, confirm }` | Bogfører udstedt faktura i finansen. |
| `invoice_post_compensation` | `invoice post-compensation` | `{ company, documentId? \| invoiceNumber?, date?, confirm }` | Bogfører registreret kompensation. |
| `invoice_post_interest` | `invoice post-interest` | `{ company, documentId? \| invoiceNumber?, claimId?, date?, confirm }` | Bogfører registreret morarentekrav. |
| `invoice_post_reminder` | `invoice post-reminder` | `{ company, documentId? \| invoiceNumber?, reminderId?, date?, confirm }` | Bogfører registreret rykker. |
| `invoice_refund_bank` | `invoice refund-bank` | `{ company, payload: RefundPayload, confirm }` | Bogfører refundering til kunde fra banken. |
| `invoice_remind` | `invoice remind` | `{ company, documentId? \| invoiceNumber?, date, fee?, note?, confirm }` | Registrerer rykker på forfalden faktura. |
| `invoice_render` | `invoice render` | `{ company, documentId? \| invoiceNumber?, confirm }` | Renderer (eller genskaber) deterministisk PDF. Idempotent. |
| `invoice_send_email` | `invoice send` | `{ company, documentId? \| invoiceNumber?, kind?, to?, confirm }` | Sender faktura/rykker via SMTP med PDF vedhæftet. Idempotent. SMTP-config læses fra `config/smtp.json` i virksomhedsmappen — påkrævede felter: `host`, `port`, `fromAddress`; valgfri: `fromName`, `username`, `password`, `dryRun`. Mangler filen ⇒ `{ ok:false, errors:["missing SMTP config: ..."] }`. Den indbyggede transport kører **kun** i dry-run: `dryRun:true` registrerer afsendelsen uden netværkskald (`ok:true`); uden `dryRun:true` fejler et rigtigt send med en `ok:false`-envelope. |
| `invoice_settle_bank` | `invoice settle-bank` | `{ company, payload: SettlementPayload, confirm }` | Matcher bankbetaling mod faktura. |
| `invoice_settle_claim_bank` | `invoice settle-claim-bank` | `{ company, payload: ClaimSettlementPayload, confirm }` | Matcher bankbetaling mod fakturakrav. |
| `invoice_write_off_bad_debt` | `invoice write-off-bad-debt` | `{ company, payload: BadDebtPayload, confirm }` | Bogfører tab på debitor. |
| `journal_post` | `journal post` | `{ company, payload: JournalEntryInput, confirm }` | Bogfører manuel finanspostering. |
| `journal_reverse` | `journal reverse` | `{ company, entryId? \| entryNo? \| matchText?, matchDate?, matchDocumentId?, date, reason, confirm }` | Tilbagefører bogført finanspostering ved at oprette modpost. |
| `payable_pay` | `payable pay` | `{ company, payableId, bankTransactionId, amount?, date?, paymentAccount?, note?, confirm }` | Matcher en udgående bankbetaling mod en åben kreditorpost (debit 7000 Leverandørgæld, credit bank). |
| `payable_register` | `payable register` | `{ company, documentId, billDate, dueDate, expenseAccount, vatTreatment?, vendorId?, note?, confirm }` | Registrerer et bogført leverandørbilag som en åben kreditorpost (debit udgift + købsmoms, credit 7000 Leverandørgæld). |
| `peppol_submit_public_invoice` | `invoice submit-public-peppol` | `{ company, documentId? \| invoiceNumber?, accessPoint, acknowledgement?, confirm }` | Bygger en idempotent PEPPOL-submission-envelope og registrerer forsøget. |
| `period_close` | `period close` | `{ company, from, to, kind?, status?, reference?, confirm }` | Lukker eller markerer regnskabsperiode. |
| `recurring_invoice_create` | `recurring-invoice create` | `{ company, name, interval, firstIssueDate, invoice: InvoicePayload, paymentTermsDays?, deliveryPeriodMode?, notes?, confirm }` | Opretter en gentagende fakturaskabelon. `invoice` er en typet `InvoicePayload` (samme form som `invoice_issue`) — men dato-/nummerfelter (`invoiceNumber`, `issueDate`, `dueDate`, leveringsdatoer) sættes IKKE her; `recurring_invoice_generate` udleder dem pr. periode. |
| `recurring_invoice_generate` | `recurring-invoice generate` | `{ company, templateId, asOfDate, confirm }` | Materialiserer den forfaldne faktura for skabelonen. Idempotent pr. template/periode. |
| `system_backup` | `system backup` | `{ company, at?, archive?, confirm }` | Opretter revisionsklar backup. `archive:true` pakker straks til ét `.tar`. |
| `system_backup_archive` | `system backup-archive` | `{ company, backupId?, out?, confirm }` | Pakker en eksisterende backup til ét deterministisk `.tar` (+ `.sha256`). |
| `system_backup_confirm_placement` | `system backup-confirm-placement` | `{ company, destinationId, backupId?, archiveSha256?, archiveSizeBytes?, actorKind?, at?, note?, confirm }` | Registrerer en backup-placering foretaget uden for Rentemester. |
| `system_backup_destination_add` | `system backup-add-destination` | `{ company, label, kind, location, inEeaOrEu, attestedBy, regionCountry?, regionNote?, nonRelatedParty?, itSecurityMeetsStandards?, itSecurityNote?, at?, confirm }` | Tilføjer en backup-destination med EU/EØS-attestering (BEK 205/2024 § 4). |
| `system_backup_destination_remove` | `system backup-remove-destination` | `{ company, id, confirm }` | Fjerner en konfigureret backup-destination. |
| `system_backup_lock` | `system backup-lock` | `{ company, enforced, graceDays?, at?, confirm }` | Konfigurerer den frivillige bogførings-lås. |
| `system_backup_place` | `system backup-place` | `{ company, archivePath, destinationId, actorKind?, at?, note?, confirm }` | Kopierer et backup-arkiv til en lokal/synkroniseret destination og verificerer med sha256. |
| `system_export_authority` | `system export-authority` | `{ company, from, to, out, requestedAt?, requester?, confirm }` | Eksporterer materiale til myndighedsudlevering. |
| `vat_post_eu_service_purchase` | `vat post-eu-service-purchase` | `{ company, payload: ReverseChargePurchaseInput, confirm }` | Bogfører EU-servicekøb med reverse charge. |
| `vat_post_representation_purchase` | `vat post-representation-purchase` | `{ company, payload: RepresentationPurchaseInput, confirm }` | Bogfører repræsentationsudgift med delvis momsfradrag. |

> De seks `system_backup_*`-konfigurations-tools (`*_archive`,
> `*_confirm_placement`, `*_destination_add`, `*_destination_remove`,
> `*_lock`, `*_place`) skriver state uden at bogføre i finanskæden. De
> klassificeres her som `write-irreversible` fordi de er
> `confirm`-gatede writes, men de oprettede records (destinationer,
> placeringsregistreringer, lås-konfiguration) kan rettes ved nye kald.

## System-tools

`system_*`-tools dækker healthcheck, backup-governance og restore. De er
fordelt på read- og write-tabellerne ovenfor efter klassifikation; her
fremhæves kun det destruktive tool.

| Tool | CLI-ækvivalent | Klassifikation | Input | Brief |
|---|---|---|---|---|
| `system_restore_backup` | `system restore-backup` | **destructive** | `{ backupDir, targetCompany, verifyKey?, publicKey?, confirm, confirmText? }` | Gendanner backup til en ny virksomhedssti. `confirmText` skal være `"RESTORE <targetCompany>"` præcist. Sletter intet på source, men kan overskrive filer i `targetCompany`. |

Felt-detaljer for `system_restore_backup`:

| Felt | Påkrævet | Beskrivelse |
|---|---|---|
| `backupDir` | ja | Sti til backup-mappen eller `.tar`-arkivet der gendannes fra (på MCP-serverens filsystem). Røres aldrig af restore. |
| `targetCompany` | ja | Sti til virksomhedsmappen backuppen gendannes IND i. Eksisterende filer her kan blive overskrevet. |
| `verifyKey` | nej | Sti til den **symmetriske HMAC-verifikationsnøgle** (`.backup-manifest.key`) der verificerer manifestets HMAC-tag. **Ikke** ed25519-nøglen. Typisk påkrævet for et `.tar`-arkiv; for en backup-mappe stadig i sin `backups/`-mappe udledes nøglen ellers. Spejler CLI'ens `--verify-key`. |
| `publicKey` | nej | Sti til den **asymmetriske ed25519 public key** der verificerer manifestets ed25519-signatur (signaturen `system backup --sign-with-ed25519` tilføjer). Adskilt fra `verifyKey`. Spejler CLI'ens `--public-key`. |
| `confirm` | nej (schema), men `true` kræves | `confirm: true` for at bekræfte de destruktive sideeffekter. Udeladt/`false` → `{ ok:false, errors:["confirm: true required for destructive tool system_restore_backup"] }`. |
| `confirmText` | nej (schema) | Skal være `"RESTORE <targetCompany>"` præcist. Schema-valgfri (#307): et udeladt/tomt `confirmText` returnerer den normale `{ ok:false, errors:["confirmText must match …"] }`-envelope — IKKE en rå `-32602`. |

## CLI/MCP-mapping

MCP-surface'en er *tæt på* 1:1 med CLI'en, men ikke fuldstændig. Kendte
afvigelser pr. denne revision:

- **`period_list` har ingen CLI-kommando.** Tool'et `period_list` lister
  regnskabsperioder over MCP, men CLI'en har kun `period close` — der er
  ingen `period list`-kommando. (Tidligere noterede dette dokument
  `period list` som en CLI-kommando "der skal bygges"; den er aldrig blevet
  bygget. MCP-tool'et læser `accounting_periods` direkte.) Vil man genskabe
  1:1-mappingen skal en `period list`-CLI-kommando tilføjes — ellers er
  dette en bevidst, dokumenteret afvigelse.
- **`import_archive_year` har ingen selvstændig CLI-kommando.** Den henter
  fra samme arkiv-artefakt som `import archive` skriver/lister.
- **`portfolio_overview`** dækker delvist det CLI'en eksponerer som
  `dashboard`, men er et workspace-tool (`workspace`-parameter, ikke
  `company`).
- **`period reopen` er CLI-only.** En for tidligt lukket regnskabsperiode kan
  kun genåbnes via CLI-kommandoen `period reopen` (en kontrolleret, fuldt
  revisionssporet handling, #247) — der findes *ingen* `period_reopen`
  MCP-tool. MCP-surface'en eksponerer `period_list` (read) og `period_close`
  (write-irreversible), men ikke en genåbning. En agent der over MCP rammer
  en lukket-periode-afvisning kan altså ikke selv genåbne perioden; den må
  henvise mennesket til `rentemester period reopen`.
- **`invoice create` er CLI-only — den ergonomiske CLI-vej til at fakturere.**
  `invoice create` udsteder en kundefaktura uden at man selv skriver JSON eller
  regner moms (linjer som `"beskrivelse|antal|stykpris"`, satsen i procent).
  Den har *ingen* MCP-tool: MCP-pendanten er det typede `invoice_issue`, der
  tager en fuld `InvoicePayload` med færdigberegnede totaler. En agent over MCP
  bruger altså `invoice_issue` (typet payload) — `invoice create` er
  CLI-ergonomien for et menneske.
- **`invoice export-public` og `invoice export-public-oioubl` er CLI-only.**
  De skriver deterministiske handoff-artefakter til offentlig e-faktura
  (henholdsvis et EAN/GLN-preview og et OIOUBL-handoff) uden PEPPOL-transport.
  Der findes ingen `*_export_public*` MCP-tools — MCP-surface'en eksponerer kun
  `peppol_submit_public_invoice` til den egentlige PEPPOL-submission.
- **CLI-only-kommandoer uden MCP-tool.** Flere CLI-kommandoer eksponeres
  ikke som tools, fx `init`, `serve`, `report *`, `vat momsangivelse`/
  `vat filing`, `period reopen`, `gdpr export`/`gdpr erase`,
  `opening-balance post`, `bank-account add`/`list`,
  `import run`/`systems`/`contacts`, `agent run`,
  `reg coverage`/`reg citations`, `invoice create`,
  `invoice export-public`/`invoice export-public-oioubl` og diverse
  `system export-*`/`verify-*`-kommandoer. Disse driver lokale workflows
  eller hører til den indbyggede `agent run`-loop og er bevidst holdt uden
  for den løse agent-surface.

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
    "entries": 142
  },
  "errors": []
}
```

`data` carries **only** `{ entries }` — the count of journal entries that were
verified. The integrity verdict is the **envelope** `ok`/`errors`, not a field
inside `data`: `ok:true` with an empty `errors[]` means the hash-chain and
bookkeeping integrity checks passed. A broken chain returns `ok:false` with the
specific failures in `errors[]`:

```json
{
  "ok": false,
  "errors": [
    "2026-00087: entry_hash mismatch",
    "2026-00088: previous_hash mismatch"
  ]
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

Output (forkortet — kun de centrale felter; de fulde `payments[]`/`*Claims[]`/
`total*`-felter er udeladt her, se `InvoiceStatusResult`):
```json
{
  "ok": true,
  "data": {
    "invoiceDocumentId": 87,
    "invoiceNumber": "2026-00042",
    "grossAmount": 12500.00,
    "creditedAmount": 0,
    "paidAmount": 5000.00,
    "openBalance": 7500.00,
    "asOfDate": "2026-05-18",
    "dueDate": "2026-04-30",
    "effectiveDueDate": "2026-04-30",
    "isOverdue": true,
    "overdueDays": 18,
    "status": "open"
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
        { "accountNo": "3120", "debitAmount": 320.00, "vatCode": "DK_PURCHASE_25" },
        { "accountNo": "3050", "debitAmount": 80.00 },
        { "accountNo": "2000", "creditAmount": 400.00 }
      ]
    },
    "confirm": true
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
    "verifyKey": "/Users/mikkel/keys/.backup-manifest.key",
    "publicKey": "/Users/mikkel/keys/acme-aps-backup.pub",
    "confirm": true,
    "confirmText": "RESTORE /Users/mikkel/companies/acme-aps-restored"
  }
}
```

> `verifyKey` er den symmetriske HMAC-nøgle (`.backup-manifest.key`);
> `publicKey` er den asymmetriske ed25519 public key (`*.pub`). Begge er
> valgfrie — `verifyKey` udelades typisk når backuppen restoreres fra sin
> oprindelige `backups/`-mappe; `publicKey` udelades når manifestet ikke
> har en ed25519-signatur eller nøglen ligger inde i backuppen.

Output (success):
```json
{
  "ok": true,
  "data": {
    "backupId": "2026-05-17T22-00-00Z",
    "restoredAt": "2026-05-18T09-14-02Z",
    "targetCompanyRoot": "/Users/mikkel/companies/acme-aps-restored",
    "restoredDbPath": "/Users/mikkel/companies/acme-aps-restored/ledger.sqlite",
    "restoredFiles": {
      "documentsOriginals": 41,
      "invoicesIssued": 18,
      "config": 3
    }
  },
  "appliedRules": ["DK-BOOKKEEPING-RESTORE-001"]
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

> Det destruktive `system_restore_backup` bruger ordet **destructive** i
> stedet for **write**: `confirm: true required for destructive tool
> system_restore_backup`. Match på `confirm: true required for` for at
> fange begge varianter.

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

MCP-serveren sætter actor-konteksten per kald (ikke globalt — den
serialiseres ind i kerne-funktionens input via `createdBy`/`createdByProgram`,
fordi proces-env-vars er race-prone når flere requests behandles parallelt).
Implementeringen er `deriveMcpActor()` i `src/mcp/actor.ts` — den nedenstående
tabel skal matche den.

Mapping fra MCP-handshake/miljø til `McpActor`-felterne:

| Felt | Kilde | Eksempel |
|---|---|---|
| `createdBy` | MCP-klientens `Implementation` fra initialize-handshake: `agent:<name>/<version>`. Mangler klient-navnet, bruges `agent:<RENTEMESTER_MCP_AGENT>`; er den heller ikke sat, `agent:unknown-mcp-client`. | `agent:claude-code/0.4.1` |
| `createdByProgram` | `mcp:<RENTEMESTER_MCP_USER>` hvis env-varen er sat (typisk login-email); ellers fallback-strengen `rentemester-mcp`. | `mcp:mikkel@56n.dk` |
| `auditActor` | Beregnet `"<createdBy> via <createdByProgram>"`. | `agent:claude-code/0.4.1 via mcp:mikkel@56n.dk` |

Bemærk: klient-versionen er en del af `createdBy` (`agent:<name>/<version>`),
ikke et separat felt. Brugeren stammer fra `RENTEMESTER_MCP_USER`-miljøvariablen
på serverens host og ender i `createdByProgram` — der findes ingen
`userContext`-parameter på selve tool-kaldet. Mangler `RENTEMESTER_MCP_USER`,
er `createdByProgram` blot `rentemester-mcp`. `mcp-install.md` beskriver det
samme.

Hver write-tool tilskriver derudover automatisk:
- `audit_log.event_type` = tool-navn (`journal_post`, `invoice_issue`, …)
- `audit_log.actor` = `auditActor`
- `audit_log.entity_type` + `entity_id` = den primære nyligt oprettede entitet.

## Forudsætninger

Disse forudsætninger lå til grund for MCP-implementationen og er nu på plads:

1. **Dependencies i `package.json`**: `@modelcontextprotocol/sdk` (MCP-server
   runtime) og `zod` (input-validering).
2. **Tool-registret som single-source-of-truth**: `src/mcp/registry.ts`
   registrerer hele surface'en pr. domæne. `tests/unit/mcp-tool-surface.test.ts`
   verificerer at dette dokument holdes synkront, og
   `tests/unit/mcp-server.test.ts` driver en kørende server og verificerer
   den faktiske tool-liste.
3. **Strukturerede output-typer**: kernens `*Result`-typer wrappes i
   `{ ok, data, errors, appliedRules }` af en lille adapter i MCP-laget.
4. **`confirmText`-håndhævelse på destructive tools** via en helper i
   MCP-laget; det destruktive `system_restore_backup` afvises uden korrekt
   `confirmText`.

### Åbne afvigelser (ikke en forudsætning, men en bevidst gæld)

- Den oprindelige plan om en ny `period list`-CLI-kommando blev aldrig
  realiseret. `period_list` lever som MCP-tool uden CLI-pendant; se
  "CLI/MCP-mapping". Dette er en accepteret afvigelse fra "1 tool = 1
  CLI-kommando"-princippet, ikke en fejl der blokerer noget.
