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
   `imap_intake_*`, `efaktura_*`, `peppol_*`, `company_*`, `portfolio_*`, `import_*`).
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
     Markeret med `annotations.readOnlyHint: true`. Company-scoped reads åbner
     kun en eksisterende SQLite snapshot i læsetilstand: de initialiserer eller
     migrerer aldrig ledgeren og skriver aldrig WAL/SHM, audit, profil eller
     workspace-manifest. Manglende, uinitialiserede og schema-pending ledgers
     returnerer en afgrænset fejl-envelope uden filsystemændringer.
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
6. **Idempotency-key på fem højrisiko-writes.** `journal_post`,
   `journal_reverse`, `expense_book`, `payable_register` og `payable_pay`
   accepterer en klientgenereret nøgle på højst 128 tegn. Den kræver
   autentificeret bruger/servicekonto og scope’er på stabil principal,
   workspace, virksomhed og operation — aldrig actor eller credential. Nøglen
   hashes, udfaldet og audit commit’er atomisk. Efter 30 dage kan udfaldet
   slettes, men tombstonen bevares og afviser med
   `IDEMPOTENCY_OUTCOME_EXPIRED`; nøglen kan ikke genbruges.
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
| `write-reversible` | `confirm: true` | `customer_create`, `vendor_create`, `bank_import`, `documents_ingest`, `documents_set_company_context`, `exception_resolve`, `mileage_log` |
| `write-irreversible` | `confirm: true` | `accounts_add`, `journal_post`, `invoice_issue`, `invoice_post`, `expense_book`, `vat_post_*`, `asset_register`, `system_backup` |
| `destructive` | `confirm: true` + `confirmText` | `system_restore_backup` |

`journal_reverse` er klassificeret som `write-irreversible`: den skriver en ny
post i den append-only kæde — den modposterer en tidligere post, men kæden
selv ændres ikke.

## Resultat-shapes (`outputSchema`)

**Alle 225 tools deklarerer et `outputSchema`** (#202). Det er det samme
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
| `code` | `string` | Valgfri. Stabil maskinlæsbar fejl-markør for cross-cutting preconditions (fx `"BACKUP_LOCKED"`) — se "Cross-cutting preconditions" nedenfor. Sættes kun ved `ok:false`; udeladt for per-tool forretningsfejl. |

`outputSchema` typer bevidst `data` som et **åbent objekt** (`passthrough`):
den konkrete feltliste i `data` varierer pr. tool, og MCP-SDK'en validerer
kun `structuredContent` mod schemaet for *succes*-svar (`isError:false`) —
fejl-envelopes springes over. De per-tool `data`-felter er ikke hånd-typet
120 gange; de er dokumenteret nedenfor og i tool-brief'ene.

### Cross-cutting preconditions (envelope-`code`)

Visse forudsætninger gælder *på tværs* af tool-surfacen og lever derfor uden
for det enkelte tools `inputSchema`. Når en sådan precondition slår til,
fejler tool'et med en konvolut der bærer en **stabil `code`-markør** så en
agent kan branche uden at parse `errors[]`-strengen.

| `code` | Trigger | Hvilke tools rammes | Recovery |
|---|---|---|---|
| `CONFIRM_REQUIRED` | Et write- eller destructive-tool blev kaldt uden `confirm: true`. Tekst-varianten i `errors[]` er `confirm: true required for write tool <name>` hhv. `… destructive tool system_restore_backup` — men `code` er den samme for begge. | Alle confirm-gatede tools (`write-reversible` + `write-irreversible` + `destructive`). | Re-call med `confirm: true` (kun når forudsætningerne er på plads — confirm er en beslutning, ikke en formalitet). |
| `CONFIRMTEXT_MISMATCH` | `system_restore_backup` blev kaldt med manglende/tomt **eller** forkert `confirmText` (#307 — begge giver denne envelope, aldrig en rå `-32602`). | Kun `system_restore_backup`. | Send `confirmText: "RESTORE <targetCompany>"` eksakt. |
| `BACKUP_LOCKED` | Backup-pligten er forsømt og `system_backup_lock` er sat til `enforced:true` med overskredet grace-periode. | **Alle** skrive-tools (`write-reversible` + `write-irreversible`) bortset fra `system_*`-familien. Read-tools og `system_*`-tools er aldrig lås-gated — at låse op kræver `system_backup`, så det skal altid kunne kaldes. | 1) `system_backup_status` for at se hvor langt forsinkelsen er. 2) `system_backup` (med `archive:true`) for at producere arkivet og frigøre låsen. 3) `system_backup_place` for at placere arkivet på en EU/EØS-attesteret destination. |
| `ACTOR_NOT_ALLOWED` | Den afledte MCP-actor består ikke virksomhedens `config/policy.yaml` `actor_allowlist` (SEC-2 — samme gate som CLI'en). | Alle confirm-gatede skrive-tools. | Tilføj actoren under `actor_allowlist.agents` i `config/policy.yaml`, eller kald fra en allerede tilladt klient. |

De følgende tre koder (AGENT-16) markerer de **hyppigste tværgående
forretningsfejl**. De afledes i konvolut-laget (`wrapCoreResult` /
`errorEnvelope`) fra kernens fejl-tekst, så en agent kan branche på `code`
i stedet for at mønster-matche fri-tekst. En forretningsfejl der ikke matcher
nogen af dem bærer **ingen** `code` og er kun beskrevet i `errors[]`.

| `code` | Trigger | Hvilke tools rammes | Recovery |
|---|---|---|---|
| `PERIOD_CLOSED` | En postering har en `transaction_date` der falder i en lukket/rapporteret regnskabsperiode (`... falls in closed/reported period ...`). | Alle bogførings-tools der skriver med en dato (`journal_post`, `invoice_post`, `vat_*`, `asset_*`, …). | Vælg en dato uden for den lukkede periode, eller genåbn perioden bevidst (`period_reopen`) før genbogføring. |
| `PRECONDITION_MISSING` | En livscyklus-forudsætning mangler — fx skal en faktura være bogført (`invoice_post`) før den kan afregnes/krediteres. `errors[]` starter med `Forudsætning ikke opfyldt:` og navngiver det tool der skal køres først. | Settlement-/credit-note-/rykker-/rente-/kompensations-familien for udstedte fakturaer. | Kør det navngivne forudgående tool (typisk `invoice_post`), og gentag derefter kaldet. |
| `NOT_FOUND` | En refereret entitet findes ikke (fx `invoice document N does not exist`, ukendt bank-transaktion, ukendt bilag). | Ethvert tool der slår en id/entitet op. | Find den rigtige id via et discovery-tool (`invoice_list` / `invoice_find` / `bank_list`, …) og gentag kaldet. |
| `NOT_VAT_REGISTERED` | Selskabet er IKKE momsregistreret (`company_profile_get.vatRegistered` er `false` / `vatPeriodType` er `null`), og et moms-tool blev kaldt (`errors[]`: `selskabet er ikke momsregistreret`). | `vat_report`, `vat_eu_sales_list`, `vat_oss_report` og de to `vat_post_*`-skrive-tools (samt `expense_book` med `reverse_charge`/`representation`). Momsangivelse, -frister og købsmoms-fradrag findes ikke for et ikke-registreret selskab. | Producér ingen momsangivelse. Bogfør i stedet bilag med moms via `expense_book`/`payable_register` med `vat_treatment: "non_deductible"`, så momsen absorberes i udgiften (§ 37). Skal selskabet alligevel momsregistreres, så sæt en kadence med `company set-profile --vat-period …` først. |

`errors[]` på disse konvolutter indeholder en menneskelæsbar forklaring
(på `BACKUP_LOCKED` dansk tekst + det afviste tool-navn og
`system_backup_status` som første næste-skridt). Maskinlæsbar branching
skal dog ske på `code`, ikke på fri-tekst — strengene er kun fallback for
klienter der ikke læser `code` (se `docs/confirm-contract.md`).

### `data`-felter pr. tool — det der har betydning

`read`-tools returnerer typisk en liste plus en tæller. Seks list-tools er
**paginerede** (#381): `journal_list`, `bank_list`, `customer_list`,
`vendor_list`, `documents_list` og `audit_log_list` tager valgfri
`limit`/`offset` i input (default `limit` 500, hard-cap 5000 — over cap
afvises på zod-niveau) og returnerer paginerings-metadata i `data`:
`total` (alle matchende rækker), `count` (rækker i dette svar), `limit`,
`offset`, `hasMore` og — når `hasMore=true` — `nextOffset`, som agenten kan
sende uændret for at hente næste side. Et svar med `hasMore: true` er
**ikke** komplet — en agent der ignorerer det mister stiltiende rækker.

| Tool(s) | `data`-felter |
|---|---|
| `accounts_list` | `{ accounts: [{ accountNo, name, type, defaultVatCode }], count }` |
| `accounts_roles_status` | `{ status, missing[], ambiguous[], proposals[], candidates[], reasons[], roles[] }`; forslag er read-only og bliver aldrig implicitte posting-defaults. |
| `journal_list` | `{ entries: [{ id, entryNo, transactionDate, text, currency, amountForeign, amountDkk, fxRateToDkk, documentId, sourceBankTransactionId, status, reversalOfEntryId }], total, count, limit, offset, hasMore, nextOffset? }` (pagineret) |
| `journal_dry_run` | `{ entryId, entryNo, previousHash, entryHash, accountEffects: [{ accountNo, accountName, balanceBefore, balanceAfter, delta }] }` — ikke-bindende forhåndsvisning af `journal_post`: felterne beskriver hvad posteringen *ville* få. `accountEffects` lister saldo før/efter pr. berørt konto (debet-minus-kredit-netto, i kroner). Ved en ugyldig payload er konvolutten `ok=false` med `errors[]`, og `data` mangler. |
| `bank_list` | `{ rows: [...], total, count, limit, offset, hasMore, nextOffset? }` (pagineret) |
| `invoice_list` | `{ invoices: [...], count }` |
| `invoice_imported_receivables` | `{ asOfDate, boundary, count, totalOpen, rows: [{ externalInvoiceId, invoiceDate, grossAmount, paidAmount, openBalance, controlAccountNo, sourceDocumentHash, scheduleHash }] }` — source-evidenced pre-cut-over debtors only. They are intentionally separate from `invoice_list`; never add the two without an explicit reconciliation. |
| `exceptions_list` | `{ exceptions: [...], count }` |
| `period_list` | `{ periods: [{ id, periodStart, periodEnd, kind, status, reference, createdAt }], count }` — `kind` er `"vat_period" \| "fiscal_year" \| "custom"`; ældre rækker kan læses som `"vat_quarter"`, der kun er et legacy-alias. `status` er `"open" \| "closed" \| "reported"`; `reference` kan være `null`. |
| `audit_verify` | `{ entries: <number> }` — kun antallet af verificerede posteringer. Integritets-verdikten læses fra **konvolutten**: `ok=true` (+ tom `errors[]`) ⇒ kæden er intakt; `ok=false` ⇒ `errors[]` lister bruddene. Der er hverken `ok` eller `errors` *inde i* `data`. |
| `gdpr_discover` | `{ subject: { cvr, name }, rows: [{ source, sourceRowId, label, personalData, retainUntil, erased }], byTable: { customers, vendors, documents, bank_transactions, journal_entries, journal_lines, audit_log } }` — tombstone-overlayet gælder også discovery; opslaget append-only audit-logges og kræver derfor `confirm:true`. |
| `gdpr_export` | `{ asOf, subject: { cvr, name }, records: [{ source, sourceRowId, label, personalData, retainUntil, underRetention, erased, erasable }] }` — `erasable=false` for retention, allerede slettede rækker og hash-kædede journaldata. DSAR-eksporten append-only audit-logges og kræver derfor `confirm:true`. |
| `gdpr_audit_log` | `{ format, ruleId, asOf, since, until, events: [{ id, occurredAt, eventType, subjectKey, actor, message }], canonicalPayload, fingerprint, signature? }`; `canonicalPayload` er de eksakte UTF-8-bytes som fingerprint/signatur dækker. `signature` findes kun når `signWithEd25519:true` og et komplet, matchende backup-nøglepar kan læses. |
| `invoice_status` | `{ invoiceDocumentId, invoiceNumber, grossAmount, creditedAmount, paidAmount, openBalance, claimOpenBalance, asOfDate, dueDate, effectiveDueDate, isOverdue, overdueDays, status, payments[], creditNotes[], refunds[], claimPayments[], badDebtWriteOffs[], reminders[], compensationClaims[], interestClaims[], interestCorrections[], totalReminderFees, totalCompensationClaims, totalInterestClaims, totalInterestCorrections, totalClaimPayments, totalBadDebtWrittenOff }` — feltet hedder `invoiceDocumentId` (ikke `documentId`), `invoiceNumber` (ikke `invoiceNo`) og `overdueDays` (ikke `daysOverdue`). `status` er `"open" \| "paid" \| "credited" \| "refunded" \| "overpaid" \| "written_off"`. Den fulde typedefinition er `InvoiceStatusResult` i `src/core/invoice-payments.ts`. |

`write`-tools returnerer id'er + hashes på den nyligt oprettede entitet:

| Tool | `data`-felter |
|---|---|
| `accounts_role_confirm` | `{ resolution, status, missing[], ambiguous[], proposals[], candidates[], reasons[], roles[] }`; `resolution` indeholder konto, version, actor og confirmation-proveniens. |
| `journal_post` | `{ entryId, entryNo, entryHash }` |
| `invoice_issue` | `{ documentId, invoiceNumber, storedPath, sha256, pdfDocumentId?, pdfStoredPath?, pdfSha256? }` — feltet hedder `documentId` (ikke `invoiceDocumentId`); `invoiceNumber` (ikke `invoiceNo`). |
| `customer_create` / `vendor_create` | `{ customerId }` / `{ vendorId }` |
| `accounts_add` | `{ accountNo }` |
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
| `efaktura_konfigurer` | `{ configPath, environment }` — `configPath` er stien til den skrevne secret-fil (config/digisense.json); `environment` er `"production" \| "test"`. license-key returneres ALDRIG. |
| `efaktura_registrer` | `{ companyKey, directionsRegistered, network, participantType, participantId }` — `companyKey` er Digisense' nøgle for virksomheden; `directionsRegistered` er de registrerede retninger (fx `["inbound","outbound"]`); `network` er `"nemhandel" \| "peppol"`; `participantType` er `"DK:CVR" \| "GLN"`. |
| `efaktura_onboarding_status` | Secret-redacted local readiness: profile identity, configured environment, inbound/outbound readiness and blockers. |
| `efaktura_onboard` | `{ companyKey, status }` — validates auth and idempotently registers the profile CVR inbound + outbound. |
| `efaktura_modtag` | `{ pagesFetched, documentsListed, documentsIngested, documentsSkipped, documentsQuarantined, documents[], errors[] }` — tællere over pollen; `documentsQuarantined` er TERMINALT uingesterbare bilag (validering/dublet) der er sat i karantæne så de ikke down­loades igen. `documents[]` er `ReceivedDocumentOutcome`: `{ internalId, status, documentNo?, errors? }`, hvor `status` er `"ingested" \| "skipped-duplicate" \| "quarantined" \| "error"`. **Partiel succes:** konvoluttens `ok` er kun `false` ved en BATCH-fejl (list-received-documents fejlede); en enkelt dårlig faktura giver `ok:true` med tællerne intakte og fejlen i `documents[]`/`errors[]`. |
| `efaktura_modtag_workspace` | `{ companies: [{ slug, status, reason?, documentsIngested? }] }` — confirm-gated poll af aktive manifest-virksomheder. Ingen caller credentials/companyKey; arkiverede og ukonfigurerede springes over, fejl fortsætter pr. virksomhed, og resultater er redigerede. |
| `efaktura_send` | `{ invoiceNumber, submissionReference, idempotencyKey, status, transmissionId, ... }` — samme `SubmitPublicEInvoicePeppolResult`-shape som `peppol_submit_public_invoice`. `status` er `"acknowledged"` ved levering. En queued-men-endnu-ikke-leveret accept giver `ok:true` + `status:"prepared"` + `transmissionId`; klienten skal kun polle status og aldrig redelivere. En terminal afvisning efter accept giver `status:"failed"`; et tvetydigt delivery-POST uden pålideligt remote-id giver `status:"uncertain"`. Begge blokerer redelivery, og CLI'en afslutter med exit 1. |
| `efaktura_status` | `{ invoiceNumber, submissionReference, idempotencyKey, status, transmissionId, ... }` — observerer kun `document-status` for en allerede køsat afsendelse og gemmer append-only statusevidens; kalder aldrig `document-delivery`. En terminal afvisning returneres eksplicit som `status:"failed"`; CLI'en afslutter med exit 1. |

> **Discovery-kontrakten:** Konvolut-formen er maskin-kendt via `outputSchema`
> i `tools/list`. Den præcise `data`-feltliste står her og i kildens
> `*Result`-typer (`src/core/*.ts`, fx `IssueInvoiceResult`,
> `JournalEntryResult`). En agent behøver derfor ikke kalde et tool blot for
> at lære dets resultat-shape at kende.

## Tool-count summary

Tallene gælder en kørende `src/mcp/server.ts` (verificeret via `tools/list`).
Tabellerne nedenfor er den autoritative liste pr. tool — bliver prosa-tal og
tabel uenige, er det tabellerne (og i sidste ende `tools/list`) der gælder.

- **Read-tools**: 89
- **Ordinary write-tools**: 120
- **Destructive**: 1 (`system_restore_backup`)
- **Total**: **225** (read and write tool counts are verified from the live registry in CI)

## Read-tools

### PDF parsing evidence

`documents_parse_status` and `documents_parsed_text` are read-only. The latter
is capped at ten pages and returns decoded text plus a `layoutHash` per page,
never layout coordinates, stored paths, child stderr, or worker details. The
registered source file is re-snapshotted before either response. A failed
evidence check returns `{ ok:false, code:"PDF_EVIDENCE_TAMPERED",
errors:["PDF_EVIDENCE_TAMPERED"] }` without paths or diagnostics.
paired writes are `documents_parse` and `documents_parse_pending`; both require
`confirm:true`, the normal actor allow-list, and return bounded summaries only.
Parsing is evidence, not bookkeeping authority.

50 tools i den kuraterede tabel nedenfor. Ingen state-bivirkninger; må kaldes
frit og parallelt.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `accounts_list` | `accounts list` | `{ company }` | Lister kontoplanen. |
| `expense_vat_preflight` | `expense vat-preflight` | `{ company, documentId }` | Ren dry-run: afledt region, krævet validering, cache-friskhed, sikker evidens/exception og om apply ville kalde provider. |
| `accounts_roles_status` | `accounts roles-status` | `{ company }` | Viser bekræftede kontoroller, importforslag, tvetydigheder og den read-only posting-resolution. |

Document-party resolution uses exactly one visible state per document: `resolved`, `internal_no_external_party`, or `unresolved`. `documents_internal_no_external_party` is limited to internal vouchers and records a confirmed, actor-audited, hash-bound append-only decision; it never mutates document bytes, VAT, or journals.
| `accrual_register_report` | `accrual register-report` | `{ company }` | Register af periodeafgrænsningsposter med bogførte perioder, periodiseret beløb og resterende balanceeksponering. |
| `asset_register_report` | `asset register-report` | `{ company }` | Aktivregister med akkumulerede afskrivninger og bogført værdi. |
| `audit_log_list` | `gdpr audit-log` (delvis) | `{ company, fromDate?, toDate?, eventTypeLike?, actorLike?, limit?, offset? }` | Filtreret, pagineret read af audit_log — den menneskelæsbare revisionsspor over hvad agenten/cockpittet/CLI'en har gjort. Append-only på server-siden. |
| `audit_verify` | `audit verify` | `{ company }` | Verificerer hash-chain og bogføringsintegritet. |
| `bank_account_list` | `bank-account list` | `{ company, includeInactive? }` | Lister registrerede bankkonti (slug, navn, valuta, IBAN, aktiv). Den slug, der returneres her, er den værdi en agent kan sende som `account` til `bank_import` og `bank_list`. |
| `bank_list` | `bank list` | `{ company, status?, from?, to?, textMatch?, amount?, account?, limit?, offset? }` | Lister importerede banktransaktioner med filtre. Pagineret (default 500, cap 5000) — `data` har `total`/`hasMore`/`nextOffset`. |
| `bank_suggest_matches` | `bank suggest-matches` | `{ company, bankTransactionId?, max? }` | Foreslår deterministiske match mellem uafstemte bank-poster og bilag. |
| `company_profile_get` | `company profile` | `{ company }` | Henter virksomhedens gemte profil-stamdata (navn, CVR, valuta, land, adresse, regnskabsår-start, betalingsfrist, momsperiode). Hver fakturering, momsrapport og årsrapport bygger på disse felter. |
| `meta_about` | (ingen — kun MCP) | `{}` | Server-identifikation: serverName, serverVersion, antallet af registrerede tools, rules-bundle-versionen og repo-relative stier til kontrakt-dokumenter. Bruges af agenten lige efter `initialize` for at verificere identitet/version. |
| `budget_forecast` | `budget forecast` | `{ company, startDate, months }` | Likviditetsprognose: fremskriver banksaldoen måned for måned ud fra primosaldo, åbne fakturaer der forfalder, planlagte gentagne fakturaer og budgetterede omkostninger. Rent deterministisk. |
| `liquidity_forecast_13_week` | — | `{ company, startDate, weeks? }` | Read-only 13-week forecast with source buckets. `closingCash` is canonical-base cash; `scenarioClosingCash` adds dated reviewed assumptions. Undated account budgets remain informational, VAT is canonical only when filing-safe/closed-or-reported and settlement state stays explicit. Foreign currency is excluded unless a dated FX source is supplied. |
| `supplier_commitment_plan` | `supplier-commitment plan` | `{ company, commitment }` | Strict read-only proposal from source references; a recurring bank pattern is never a contract. |
| `supplier_commitment_apply` | `supplier-commitment apply` | `{ company, commitment, payloadHash, confirm, idempotencyKey? }` | Appends reviewed planning evidence only; never creates a payable, journal, payment or supplier message. |
| `supplier_commitment_list` | `supplier-commitment list` | `{ company }` | Lists active immutable commitment revisions and their hashes. |
| `supplier_commitment_change` | `supplier-commitment change` | `{ company, commitmentId, action, reason, confirm }` | Appends pause/end/supersession history without deleting earlier occurrences. |
| `supplier_commitment_match` | `supplier-commitment match` | `{ company, commitmentId, occurrenceDate, evidence, confirm }` | Confirmed append-only evidence link to a canonical document, payable, or bank transaction. It never posts or settles anything; FX comparisons are explicitly unsupported. |
| `supplier_commitment_matches` | `supplier-commitment matches` | `{ company, commitmentId? }` | Reads recorded evidence links and their deterministic amount/date/currency/party/documentation variance. |
| `supplier_commitment_alerts` | `supplier-commitment alerts` | `{ company, asOf }` | Reads renewal and notice alerts in the next 30 days. |
| `budget_list` | `budget list` | `{ company, period?, accountNo? }` | Lister de gældende (seneste-revision) budgetlinjer. |
| `budget_vs_actual` | `budget vs-actual` | `{ company, from, to }` | Sammenligner budget mod faktisk bogføring pr. konto pr. måned. |
| `customer_list` | `customer list` | `{ company, archived?, limit?, offset? }` | Lister kendte kunder. Pagineret. |
| `documents_list` | `documents list` | `{ company, limit?, offset? }` | Lister gemte bilag. Pagineret. |
| `efaktura_onboarding_status` | `efaktura onboarding-status` | `{ company }` | Lokal, secret-redacted DigiSense-readiness for ledgerens profil; foretager ingen netværkskald og returnerer aldrig API-nøgle eller signature secret. |
| `exceptions_list` | `exceptions list` | `{ company, status?, includeArchived? }` | Lister exceptions-køen (open/resolved/all). |
| `import_archive_list` | `import archive` | `{ company, sourceSystem? }` | Lister pre-cut-over regnskabsår arkiveret fra et flerårigt eksport. |
| `import_archive_year` | (afledt af `import archive`)¹ | `{ company, fiscalYear, sourceSystem? }` | Henter ét arkiveret regnskabsårs fulde posteringer + saldobalance. |
| `invoice_compensation_calc` | `invoice compensation` | `{ company, documentId? \| invoiceNumber?, asOf, amountDkk? }` | Beregner kompensationskrav (uden at registrere). |
| `invoice_find` | `invoice find` | `{ company, query?, customer?, invoiceNumber?, amount?, asOf? }` | Søger fakturaer på nummer, kunde eller beløb. |
| `invoice_interest_calc` | `invoice interest` | `{ company, documentId? \| invoiceNumber?, asOf, referenceRate }` | Beregner morarente (uden at registrere). `accruedInterestAmount` er den **inkrementelle** rente der kan opkræves nu (perioden siden sidste registrerede krav, eller fra forfald hvis ingen). Ekstra felter: `priorClaimedInterest`, `totalInterestToDate`, `claimableDays`, `interestFromDate`. |
| `invoice_interest_correction_calc` | `invoice interest-correction` | `{ company, documentId? \| invoiceNumber? }` | Foreslår en korrektion af for meget opkrævet morarente (read-only). Opstår når en betaling/kreditnota er registreret med virkningsdato inde i et allerede bogført rentekravs vindue. Felter: `hasProposal`, `overClaimedAmount`, `postedInterest`, `lawfulInterest`, `alreadyCorrected`, `throughDate`. |
| `invoice_list` | `invoice list` | `{ company, status?, from?, to?, customerCvr?, customer?, invoiceNumber?, minAmount?, maxAmount?, asOf? }` | Lister udstedte fakturaer med filtre. |
| `invoice_imported_receivables` | `invoice imported-receivables` | `{ company, asOf }` | Lister kildebeviste importerede tilgodehavender pr. cutoff. Listen er et arkiv-read og indeholder aldrig Rentemester-udstedte fakturaer. HTTP-paritet: `GET /api/companies/:slug/imported-receivables?asOf=YYYY-MM-DD`. |
| `invoice_overdue` | `invoice overdue` | `{ company, asOf?, minDays? }` | Lister forfaldne, ikke fuldt afregnede fakturaer. |
| `invoice_status` | `invoice status` | `{ company, documentId? \| invoiceNumber?, asOf? }` | Viser åben saldo og status på en faktura. |
| `invoice_validate` | `invoice validate` | `{ payload: InvoicePayload }` | Validerer faktura-payload uden at gemme. |
| `journal_dry_run` | `journal dry-run` | `{ company, payload: JournalEntryInput }` | Forhåndsviser hvad `journal_post` ville gøre — uden at skrive. Intet journalnummer forbruges. |
| `journal_list` | `journal list` | `{ company, from?, to?, status?, limit?, offset? }` | Lister finansposteringer med filtre på datointerval og status (`all`/`posted`/`reversed`). Pagineret (default 500, cap 5000) — `data` har `total`/`hasMore`/`nextOffset`. |
| `mileage_list` | `mileage list` | `{ company }` | Lister registrerede kørselsposter. |
| `mileage_report` | `mileage report` | `{ company, from, to }` | Deterministisk periode-rapport over kilometer og beløbsgrundlag. |
| `payable_list` | `payable list` | `{ company, status?, asOf? (legacy alias: asOfDate), supplier?, vendorId?, from?, to?, minDays? }` | Bygger kreditorlisten: åbne leverandørposter med åben saldo og forfaldsintervaller (forfaldne/ikke-forfaldne). |
| `period_list` | (ingen — kun MCP)² | `{ company }` | Lister regnskabsperioder (open/closed/reported). |
| `portfolio_overview` | `dashboard` (delvist) | `{ workspace, asOf? }` | Status side om side for hver virksomhed i workspace'et. Intet konsolideres. |
| `cfo_analytics_query` | `report analytics` | `{ workspace, scope, from, to, companySlug?/companySlugs?/groupProfileId?, filters?, cursor?, limit? }` | Versioneret, læsende analyse med journal-/arkivkilder. Portfolio er tydeligt ikke-konsolideret; gruppe bruger kun godkendt konsolideringsprofil. |
| `reconcile_bank` | `reconcile bank` | `{ company, from, to, status?, textMatch?, amount?, account? }` | Bygger bank-afstemningsrapport for periode. |
| `bank_reconciliation_correction_plan` | `bank correction-plan` | `{ company, bankTransactionId, replacementJournalEntryId }` | Read-only, deterministisk plan med den aktuelle afstemningsidentitet og plan-hash. |
| `bank_reconciliation_correction_apply` | `bank correction-apply` | `{ company, bankTransactionId, replacementJournalEntryId, expectedReconciliationId, planHash, reason, idempotencyKey, confirm }` | Supersederer atomisk kun den reviewede afstemning; journaler og historiske links ændres aldrig. |
| `direct_bank_purchase_payable_correction_plan` | `bank direct-payable-plan` | `{ company, documentId, bankTransactionId, billDate, dueDate, expenseAccountNo, ... }` | Read-only plan der binder dokument-, konto-, VAT-, periode- og bankevidens til `planHash`. |
| `direct_bank_purchase_payable_correction_apply` | `bank direct-payable-apply` | `{ ..., planHash, reason, idempotencyKey, confirm }` | Flytter append-only et direkte bankkøb til payable og betaler på den autoritative bankdato; læs status før retry med en ny nøgle. |
| `recurring_invoice_list` | `recurring-invoice list` | `{ company, includeInactive? }` | Lister gentagende fakturaskabeloner. |
| `recurring_invoice_run_workspace` | `recurring-invoice run-workspace` | `{ workspace, asOfDate, confirm }` | Eksplicit scheduler-kørsel for aktive manifestvirksomheder. Ingen indbygget cron; arkiverede/uinitialiserede springes over og resultater er secret-frie. |
| `retention_status` | `retention status` | `{ company, asOf? }` | Viser opbevaringsfrister og udløbet materiale. |
| `gdpr_audit_log` | `gdpr audit-log` | `{ company, since?, until?, asOf?, signWithEd25519? }` | Eksporterer GDPR-hændelser med deterministisk fingerprint og valgfri signatur; skriver ikke state. |
| `system_backup_destination_list` | `system backup-destinations` | `{ company }` | Lister konfigurerede backup-destinationer med attestering. |
| `system_backup_governance` | `system backup-governance` | `{ company, asOf? }` | Samlet backup-status: forfald, lås, destinationer, sikker placering. |
| `system_backup_status` | `system backup-status` | `{ company, asOf? }` | Tjekker om backup-pligten er opfyldt. |
| `system_healthcheck` | `system healthcheck` | `{ company }` | Read-only integritetstjek. `company` accepterer både workspace-slug og sikker sti; resultatet indeholder `checks`, `missing` og `schema` (inkl. ventende migrationsidentiteter). Stifejl redigeres før de vises til kaldende agent. |

`system migrate --company <slug|path> [--apply yes]` er bevidst CLI-only og har ingen MCP-pendant. Uden `--apply yes` er den en read-only schema-preflight; `--apply` accepterer kun den eksakte værdi `yes` og kræver en allowlistet actor.
| `tax_return_prepare` | `report tax` | `{ company, from, to }` | Forbereder selskabets skattepligtige indkomst (oplysningsskema) for et lukket regnskabsår: årets resultat + deterministiske skattemæssige reguleringer + 22% selskabsskat (kun ApS). Ikke-deterministiske poster markeres som needs-review. |
| `vat_eu_sales_list` | `vat eu-sales-list` | `{ company, from, to }` | EU-salg uden moms-liste (VIES recapitulative statement): værdien af grænseoverskridende B2B-salg uden dansk moms grupperet pr. køber-VAT-nummer. |
| `vat_oss_report` | `vat oss-report` | `{ company, from, to }` | OSS-rapportskelet (One Stop Shop, første slice): grundlaget for digitale ydelser solgt til EU-forbrugere. Ikke en OSS-indberetning til SKAT. |
| `vat_report` | `vat report` | `{ company, from, to }` | Bygger momsrapport for perioden. |
| `vendor_list` | `vendor list` | `{ company, archived?, limit?, offset? }` | Lister kendte leverandører. Pagineret. |

¹ `import_archive_year` har ingen selvstændig CLI-kommando; den hentes fra
samme arkiv-artefakt som `import archive` skriver.
² `period_list` — se "CLI/MCP-mapping" nedenfor.

> **`customer_validate_vat` — read/write-klassifikation.** Tool'et slår et
> EU-VAT-nummer op mod VIES og *skriver* resultatet til en lokal cache-tabel
> (`vies_validations`). Det er derfor klassificeret `write-reversible`
> (`readOnlyHint: false`) og kræver `confirm:true`. Den skriver ikke i
> finanskæden eller stamdata, og et gentaget opslag inden for TTL genbruger
> blot cachen (`idempotentHint: true`).
>
> MCP og CLI klassificerer begge cache-opdateringen som en bekræftet mutation;
> der er ingen read-only undtagelse for VIES-cachen.

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

17 tools. Opretter state der kan tilbageføres/arkiveres uden at røre den
append-only finanskæde.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `accounts_role_confirm` | `accounts role-confirm` | `{ company, role, accountNo, confirm }` | Bekræfter eksplicit ét kompatibelt kontorolle-forslag med actor- og versionsspor; senere confirmation kan ændre mappingen. |
| `bank_import` | `bank import` | `{ company, csvPath \| csvContent, account?, profile?, confirm }` | Importerer banktransaktioner fra CSV. Se den kanoniske [idempotenskontrakt](bank-import-idempotency.md). |
| `budget_set` | `budget set` | `{ company, accountNo, period, amount, notes?, confirm }` | Sætter et budget for én konto i én kalendermåned. Append-only revisioner — seneste vinder. |
| `dimension_assignment_replace` | `dimensions replace` | `{ company, journalLineId, expectedAssignmentId, allocations, planHash, reason, idempotencyKey?, confirm }` | Erstatter atomisk den forventede aktuelle dimensionsklassifikation med den eksakte reviewede plan; journalen ændres aldrig, og der opstår ingen ubeskyttet mellemtilstand. |
| `company_sync_cvr` | `company sync-cvr` | `{ company, confirm }` | Henter virksomhedens stamdata fra CVR og opdaterer companies-rækken. Regnskabsåret røres ikke. |
| `customer_validate_vat` | `customer validate-vat` | `{ company, cvr, confirm }` | Validerer EU-VAT via VIES og opdaterer den lokale cache. |
| `cvr_lookup` | `customer cvr-lookup` | `{ company, cvr, confirm }` | Slår en dansk virksomhed op i CVR-registret og cacher snapshottet. Kræver `CVR_USERNAME`/`CVR_PASSWORD`. |
| `customer_create` | `customer create` | `{ company, input: CreateCustomerInput, fromCvr?, confirm }` | Opretter append-only kundepost. Kan arkiveres. |
| `documents_ingest` | `documents ingest` | `{ company, filePath, metadata: DocumentMetadata, vendorId?, force?, confirm }` | Indlæser og hash-lagrer et bilag. `internal_voucher` kræver bank-id, begrundelse og moms 0. |
| `documents_set_company_context` | `documents set-company-context` | `{ company, documentId, sourceReference, businessUseReason, confirm }` | Gemmer append-only, hash-bundet virksomheds- og forretningskontekst for et dansk forenklet købsbilag eller et ufuldstændigt standardkøbsbilag; ændrer aldrig modtagerfelter på fakturaen og er ikke en moms-godkendelse. |
| `efaktura_modtag` | `efaktura modtag` | `{ company, digisenseCompanyKey?, limit?, maxTimestamp?, metadata?, force?, confirm }` | Poller modtagne e-fakturaer hos Digisense (pagination), ingester hvert nyt dokument. Dedup på internalId — rerun-stabil. |
| `efaktura_modtag_workspace` | `efaktura modtag-workspace` | `{ workspace, confirm }` | Poller aktive manifest-virksomheder med deres lokale bindings; ingen caller credentials/companyKey og redigerede per-company resultater. |
| `exception_resolve` | `exceptions resolve` | `{ company, id, note?, confirm }` | Markerer exception som løst. |
| `imap_intake_poll` | `imap-intake poll` | `{ company, imapHost, imapPort?, imapUsername, imapMailbox?, sinceUid?, metadata?, metadataPerMessage?, force?, confirm }` | Poller en IMAP-postkasse og videresender vedhæftninger til bilags-pipelinen. Dedup-stabil. |
| `mail_intake_ingest` | `mail-intake ingest` | `{ company, source, metadata?, metadataPerMessage?, force?, confirm }` | Indlæser en `.eml`-fil/maildrop-mappe og videresender vedhæftninger. Idempotent. |
| `mileage_export` | `mileage export` | `{ company, from, to, outputDir, confirm }` | Skriver et deterministisk eksport-artefakt (JSON + CSV) over kørselsregnskabet. |
| `mileage_log` | `mileage log` | `{ company, input, confirm }` | Tilføjer en append-only kørselspost. Skattemæssig behandling er brugerens ansvar. |
| `vendor_create` | `vendor create` | `{ company, input: CreateVendorInput, fromCvr?, confirm }` | Opretter append-only leverandørpost. |

### write-irreversible

51 tools (tæl tabellen — den er facit). Bogfører i den append-only hash-kæde eller skriver
revisionsklare/eksterne artefakter; kan kun "rulles tilbage" via en
modpostering.

| Tool | CLI-ækvivalent | Input | Brief |
|---|---|---|---|
| `accounts_add` | `accounts add` | `{ company, input: CreateAccountInput, confirm }` | Tilføjer én konto append-only; der findes ingen archive/undo. `defaultVatCode` skal være kanonisk. Oprettelsen bekræfter aldrig en kontorolle — brug separat `accounts_role_confirm`. |
| `gdpr_discover` | `gdpr discover` | `{ company, cvr?, name?, confirm }` | Finder alle rækker med persondata om den registrerede og skriver et actor-attribueret, append-only `gdpr_discover`-audit-event. |
| `gdpr_export` | `gdpr export` | `{ company, cvr?, name?, asOf?, confirm }` | Bygger en retention-annoteret DSAR og skriver et actor-attribueret, append-only `gdpr_export`-audit-event. |
| `accrual_recognize` | `accrual recognize` | `{ company, accrualId, period, date?, settlementAccountNo?, confirm }` | Indtægts-/omkostningsfører én periode af en periodeafgrænsningspost. |
| `efaktura_konfigurer` | `efaktura konfigurer` | `{ company, apiLicenseKey, environment?, confirm }` | Gemmer Digisense API license-key i secret-laget (config/digisense.json, 0600). PRECONDITION for efaktura_registrer/efaktura_modtag/efaktura_send. license-key rammer aldrig ledger'en. |
| `efaktura_onboard` | `efaktura onboard` | `{ company, confirm }` | Validerer auth og registrerer idempotent kun ledgerprofilens eget CVR inbound + outbound. Ekstern registrering gør governance-klassen write-irreversible. |
| `efaktura_registrer` | `efaktura registrer` | `{ company, cvr, companyName, network?, confirm }` | Registrerer en virksomhed i NemHandel via Digisense: register-company ⇒ gemmer companyKey ⇒ register-participant for BÅDE outbound OG inbound. webhookUrl=null (vi poller selv). Idempotent: re-run med samme CVR duplikerer ikke state. |
| `efaktura_send` | `invoice transmit-digisense` | `{ company, documentId? \| invoiceNumber?, digisenseCompanyKey?, confirm }` | Sender en udstedt offentlig e-faktura gennem Digisense: validate-document ⇒ deliver-document ⇒ poll til delivered; bogfører succes som acknowledged PEPPOL-submission. Kun den konfigurerede Digisense-identitet indgår i idempotens. |
| `efaktura_status` | `efaktura status` | `{ company, documentId, digisenseCompanyKey?, confirm }` | Genoptager en allerede køsat afsendelse med document-status alene; append-only status-evidens betyder, at en senere send ikke redeliverer. |
| `accrual_register` | `accrual register` | `{ company, accrualType, description, totalAmount, recognitionPeriods, firstRecognitionDate, resultAccountNo, registrationDate?, periodStepMonths?, balanceAccountNo?, settlementAccountNo?, documentId?, note?, confirm }` | Registrerer en periodeafgrænsningspost og bogfører registreringsposteringen. |
| `asset_depreciate` | `asset depreciate` | `{ company, assetId, period, date, confirm }` | Bogfører en periodes afskrivning. |
| `asset_register` | `asset register` | `{ company, name, category, acquisitionDate, cost, usefulLifeMonths, documentId, assetAccount, depreciationAccount, accumulatedAccount, note?, confirm }` | Registrerer et aktiv med lineær afskrivningsplan. |
| `asset_write_off` | `asset write-off` | `{ company, name, category, acquisitionDate, cost, documentId, expenseAccount, date, thresholdRuleSource, confirmImmediateWriteOff, paymentAccount?, note?, confirm }` | Bogfører straksafskrivning af et mindre aktiv. |
| `company_add` | `company add` | `{ workspace?, name, slug?, cvr?, fiscalYearStartMonth?, fiscalYearLabelStrategy?, confirm }` | Opretter en ny virksomhed under `<workspace>/<slug>/` og initialiserer ledgeren. Som ethvert write-tool kræver det `confirm: true` — uden flaget returneres `{ ok:false, errors:["confirm: true required for write tool company_add"] }` uden at noget oprettes. Udelades `workspace`, bruges miljøvariablen `RENTEMESTER_WORKSPACE` på MCP-serverens host; er den heller ikke sat, afvises kaldet med `no workspace given: pass 'workspace' or set RENTEMESTER_WORKSPACE`. **Ikke idempotent (`idempotentHint: false`):** et gentaget kald med samme `name`/`slug` *afvises* — det overskriver ALDRIG en eksisterende virksomhed. Findes der allerede en ledger på `<workspace>/<slug>/` fejler kaldet med `a company already exists at <sti>`, og et slug der allerede står i workspace-manifestet afvises ligeledes med en `ok:false`-envelope. For at oprette endnu en virksomhed med samme navn skal et nyt, unikt `slug` angives eksplicit. |
| `expense_book` | `expense book` | `{ company, documentId, bankTransactionId, expenseAccount, vatTreatment?, paymentAccount?, date?, text?, confirm }` | Bogfører leverandørudgift fra bilag + bankpost. |
| `expense_vat_preflight_apply` | `expense vat-preflight --apply yes` | `{ company, documentId, confirm }` | Actor-attribueret EU-VAT-preflight; gemmer kun sikker evidens og en resumérbar exception ved blokering. |
| `invoice_apply_payment` | `invoice apply-payment` | `{ company, payload: InvoicePaymentPayload, confirm }` | Registrerer fakturabetaling fra payload. Forudsætning: `invoice_post`. |
| `invoice_claim_compensation` | `invoice claim-compensation` | `{ company, documentId? \| invoiceNumber?, asOf, amountDkk?, note?, confirm }` | Registrerer kompensationskrav. Forudsætning: `invoice_post` (fakturaen skal være bogført og forfalden). |
| `invoice_claim_interest` | `invoice claim-interest` | `{ company, documentId? \| invoiceNumber?, asOf, referenceRate, note?, confirm }` | Registrerer morarentekrav. Forudsætning: `invoice_post` (fakturaen skal være bogført og forfalden). Et nyt krav opkræver kun renten for perioden siden sidste krav (inkrementelt — ingen dobbelt-opkrævning), så rente kan registreres ad flere omgange. |
| `invoice_credit_note` | `invoice credit-note` | `{ company, payload: CreditNotePayload, confirm }` | Udsteder kreditnota mod eksisterende faktura. Forudsætning: den oprindelige faktura er udstedt med `invoice_issue` og bogført med `invoice_post`. |
| `invoice_issue` | `invoice issue` | `{ company, payload: InvoicePayload, customerId?, confirm }` | Udsteder kundefaktura + immutable snapshot. Startpunktet for invoice-livscyklen. |
| `invoice_post` | `invoice post` | `{ company, documentId? \| invoiceNumber?, confirm }` | Bogfører udstedt faktura i finansen. Forudsætning: `invoice_issue`. |
| `invoice_post_compensation` | `invoice post-compensation` | `{ company, documentId? \| invoiceNumber?, date?, confirm }` | Bogfører registreret kompensation. Forudsætning: `invoice_claim_compensation`. |
| `invoice_post_interest` | `invoice post-interest` | `{ company, documentId? \| invoiceNumber?, claimId?, date?, confirm }` | Bogfører registreret morarentekrav. Uden `claimId` bogføres det ældste endnu ikke bogførte krav (kronologisk). Forudsætning: `invoice_claim_interest`. |
| `invoice_post_interest_correction` | `invoice post-interest-correction` | `{ company, documentId? \| invoiceNumber?, date?, reason?, confirm }` | Bogfører en korrektion af for meget opkrævet morarente (debiterer renteindtægt, krediterer tilgodehavende). Forudsætning: `invoice_interest_correction_calc` viser `hasProposal: true`. |
| `invoice_post_reminder` | `invoice post-reminder` | `{ company, documentId? \| invoiceNumber?, reminderId?, date?, confirm }` | Bogfører registreret rykker. Forudsætning: `invoice_remind`. |
| `invoice_refund_bank` | `invoice refund-bank` | `{ company, payload: RefundPayload, confirm }` | Bogfører refundering til kunde fra banken. Forudsætning: `invoice_post`. |
| `invoice_remind` | `invoice remind` | `{ company, documentId? \| invoiceNumber?, date, fee?, note?, confirm }` | Registrerer rykker på forfalden faktura. Forudsætning: `invoice_post` (fakturaen skal være bogført og forfalden). |
| `invoice_render` | `invoice render` | `{ company, documentId? \| invoiceNumber?, confirm }` | Returnerer og hash-verificerer den immutabelt udstedte PDF. Manipuleret eller manglende evidens genskabes aldrig. Idempotent. Forudsætning: `invoice_issue`. |
| `invoice_send_email` | `invoice send` | `{ company, documentId? \| invoiceNumber?, kind?, to?, confirm }` | Sender faktura/rykker via SMTP med PDF vedhæftet. SMTP har ingen provider-reconciliation-kontrakt: læs den kanoniske delivery-evidens før et retry; et nyt send må aldrig antages sikkert alene ud fra input. SMTP-config læses fra `config/smtp.json` i virksomhedsmappen — påkrævede felter: `host`, `port`, `fromAddress`; valgfri: `fromName`, `username`, `password`, `dryRun`. Mangler filen ⇒ `{ ok:false, errors:["missing SMTP config: ..."] }`. Den indbyggede transport kører **kun** i dry-run: `dryRun:true` registrerer afsendelsen uden netværkskald (`ok:true`); uden `dryRun:true` fejler et rigtigt send med en `ok:false`-envelope. |
| `invoice_settle_bank` | `invoice settle-bank` | `{ company, payload: SettlementPayload, confirm }` | Matcher bankbetaling mod faktura. Forudsætning: `invoice_post`. |
| `invoice_settle_claim_bank` | `invoice settle-claim-bank` | `{ company, payload: ClaimSettlementPayload, confirm }` | Matcher bankbetaling mod fakturakrav. Forudsætning: `invoice_post` + relevant `invoice_post_reminder` / `invoice_post_interest` / `invoice_post_compensation`. |
| `invoice_write_off_bad_debt` | `invoice write-off-bad-debt` | `{ company, payload: BadDebtPayload, confirm }` | Bogfører tab på debitor. Forudsætning: `invoice_post`. |
| `journal_post` | `journal post` | `{ company, payload: JournalEntryInput, confirm }` | Bogfører manuel finanspostering. |
| `journal_reverse` | `journal reverse` | `{ company, entryId? \| entryNo? \| matchText?, matchDate?, matchDocumentId?, date, reason, confirm }` | Tilbagefører bogført finanspostering ved at oprette modpost. |
| `payable_pay` | `payable pay` | `{ company, payableId, bankTransactionId, amount?, date?, paymentAccount?, note?, confirm }` | Matcher en udgående bankbetaling mod en åben kreditorpost (debit 7000 Leverandørgæld, credit bank). |
| `payable_register` | `payable register` | `{ company, documentId, billDate, dueDate, expenseAccount, vatTreatment?, vendorId?, note?, confirm }` | Registrerer et bogført leverandørbilag som en åben kreditorpost (debit udgift + købsmoms, credit 7000 Leverandørgæld). |
| `peppol_submit_public_invoice` | `invoice submit-public-peppol` | `{ company, documentId? \| invoiceNumber?, accessPoint, acknowledgement?, confirm }` | Bygger en idempotent PEPPOL-submission-envelope og registrerer forsøget. |
| `period_close` | `period close` | `{ company, from, to, kind?, status?, reference?, confirm }` | Lukker eller markerer regnskabsperiode. |
| `recurring_invoice_create` | `recurring-invoice create` | `{ company, name, interval: weekly|monthly|quarterly|yearly, intervalCount?, deliveryChannel?: manual|email|digisense, firstIssueDate, invoice: InvoicePayload, paymentTermsDays?, deliveryPeriodMode?, notes?, confirm }` | Opretter en gentagende fakturaskabelon. `invoice` er en typet `InvoicePayload` (samme form som `invoice_issue`) — men dato-/nummerfelter (`invoiceNumber`, `issueDate`, `dueDate`, leveringsdatoer) sættes IKKE her; `recurring_invoice_generate` udleder dem pr. periode. |
| `recurring_invoice_generate` | `recurring-invoice generate` | `{ company, templateId, asOfDate, confirm }` | Materialiserer den forfaldne faktura for skabelonen. Idempotent pr. template/periode. |
| `system_backup` | `system backup` | `{ company, at?, archive?, confirm }` | Opretter revisionsklar backup. `archive:true` pakker straks til ét `.tar`. |
| `system_backup_archive` | `system backup-archive` | `{ company, backupId?, out?, confirm }` | Pakker en eksisterende backup til ét deterministisk `.tar` (+ `.sha256`). |
| `system_backup_confirm_placement` | `system backup-confirm-placement` | `{ company, destinationId, backupId?, archiveSha256?, archiveSizeBytes?, actorKind?, at?, note?, confirm }` | Registrerer en backup-placering foretaget uden for Rentemester. |
| `system_backup_destination_add` | `system backup-add-destination` | `{ company, label, kind, location, inEeaOrEu, attestedBy, regionCountry?, regionNote?, nonRelatedParty?, itSecurityMeetsStandards?, itSecurityNote?, at?, confirm }` | Tilføjer en backup-destination med EU/EØS-attestering (BEK 205/2024 § 4). |
| `system_backup_destination_remove` | `system backup-remove-destination` | `{ company, id, confirm }` | Fjerner en konfigureret backup-destination. |
| `system_backup_lock` | `system backup-lock` | `{ company, enforced, graceDays?, at?, confirm }` | Konfigurerer den frivillige bogførings-lås. |
| `system_backup_place` | `system backup-place` | `{ company, archivePath, destinationId, actorKind?, at?, note?, confirm }` | Kopierer et backup-arkiv til en lokal/synkroniseret destination og verificerer med sha256. |
| `system_backup_verify_remote_placement` | `system backup-verify-remote-placement` | `{ company, destinationId, backupId, remoteObjectId, maxMetadataAgeMs?, actorKind?, at?, note?, confirm }` | Verificerer det kanoniske lokale `<backupId>.tar` mod remote objekt via provider-adapter før evidensen registreres som verificeret. |
| `system_export_authority` | `system export-authority` | `{ company, from, to, out, requestedAt?, requester?, confirm }` | Eksporterer materiale til myndighedsudlevering. |
| `vat_post_eu_service_purchase` | `vat post-eu-service-purchase` | `{ company, payload: ReverseChargePurchaseInput, confirm }` | Bogfører EU-servicekøb med reverse charge. |
| `vat_post_representation_purchase` | `vat post-representation-purchase` | `{ company, payload: RepresentationPurchaseInput, confirm }` | Bogfører repræsentationsudgift med delvis momsfradrag. |

Google Drive-verifikation bruger et kortlivet token fra
`RENTEMESTER_GOOGLE_DRIVE_ACCESS_TOKEN`; tokenet lagres aldrig i virksomhedens
mappe, backup, ledger eller auditlog. Udsted tokenet med `drive.file`, når
Rentemester har oprettet eller fået delt den konkrete backupfil. Hvis et
eksisterende objekt kræver den bredere `drive.readonly`, er det en særskilt
produktions-/sikkerhedsgodkendelse. Rotér tokenet i hostmiljøet og genstart
processen; fejl ved manglende eller tilbagekaldt token stopper fail-closed.

> De seks `system_backup_*`-konfigurations-tools (`*_archive`,
> `*_confirm_placement`, `*_destination_add`, `*_destination_remove`,
> `*_lock`, `*_place`) skriver state uden at bogføre i finanskæden. De
> klassificeres her som `write-irreversible` fordi de er
> `confirm`-gatede writes, men de oprettede records (destinationer,
> placeringsregistreringer, lås-konfiguration) kan rettes ved nye kald.

### Invoice lifecycle (#374)

`invoice_*`-familien er en **sekvens**. Hvert downstream-tool kræver at
en bestemt forrige tool er kørt — ellers afvises kaldet med en envelope-
fejl der starter med `Forudsætning ikke opfyldt:` og navngiver det
manglende forrige tool. En agent kan altså opdage rækkefølgen ud fra
tools/list (`description` indeholder en `Forudsætning:`-linje) og uden
at læse kildekoden.

**Happy path — én faktura fra udstedelse til lukning:**

```
invoice_issue                                  (opretter document_type='issued_invoice')
   │
   ├──► invoice_render                         (PDF; idempotent)
   │
   └──► invoice_post                           (debit 1100 Debitorer, credit 1000 + udgående moms)
            │
            ├──► invoice_settle_bank           (matcher bankbetaling mod tilgodehavende)
            │
            ├──► invoice_apply_payment         (lukker betaling uden bank-match)
            │
            ├──► invoice_refund_bank           (refundering — typisk efter kreditnota)
            │
            ├──► invoice_credit_note           (kreditnota mod den bogførte faktura)
            │
            └──► invoice_write_off_bad_debt    (afskriv tab på debitor)
```

**Forfaldne-fakturaer-grenen** (rykker + morarente + kompensation —
hver gren er to skridt: registrér først, bogfør derefter):

```
invoice_post (faktura forfalden)
   │
   ├──► invoice_remind ────────► invoice_post_reminder
   │
   ├──► invoice_claim_interest ─► invoice_post_interest
   │
   └──► invoice_claim_compensation ─► invoice_post_compensation
            │
            └──► invoice_settle_claim_bank     (kombineret eller separat bank-match
                                                af de bogførte krav)
```

**Fejlmønster en agent skal kunne genkende.** Hvis fx
`invoice_settle_bank` kaldes på en udstedt-men-ikke-bogført faktura,
returneres:

```json
{
  "ok": false,
  "errors": [
    "Forudsætning ikke opfyldt: faktura 2026-001 (documentId=42) er udstedt men ikke bogført. Kald invoice_post på fakturaen før invoice_settle_bank."
  ]
}
```

Beskeden er stabil i form: `Forudsætning ikke opfyldt:` + sætning der
slutter med `Kald <forrige-tool> ... før <nuværende-tool>`. Det samme
gælder for `invoice_post_reminder`/`invoice_post_interest`/
`invoice_post_compensation` der kræver at det tilhørende krav først er
registreret med `invoice_remind`/`invoice_claim_interest`/
`invoice_claim_compensation`.

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

MCP-surface'en er *tæt på* 1:1 med CLI'en, men **ikke fuldstændig**: der er
mindst 10 dokumenterede afvigelser fordelt på en CLI-only-liste og en
MCP-only-liste (se nedenfor). En agent der vælger MCP som primær overflade
finder altså IKKE alle CLI-only-funktionerne i `tools/list` — og omvendt.
Sektionerne nedenfor er maskinlæsbart udgivet: hver post navngiver den
underliggende kildefil i `src/cli/<x>.ts` eller `src/mcp/tools/<x>.ts`, så en
agent kan krydsverificere uden at læse kildekoden. Et CI-check
(`tests/unit/surface-diff-discoverable.test.ts`) fejler, hvis en ny
`src/cli/<x>.ts` eller `src/mcp/tools/<x>.ts` tilføjes uden at blive nævnt i
mapping-doc'en — `tæt på 1:1` er altså ikke længere et løst løfte, men en
konkret diff, der vedligeholdes pr. fil.

### MCP-only — tools uden CLI-pendant

- `src/mcp/tools/agent-discovery.ts` — `agent_capability_search` og
  `agent_workflow_describe` er read-only runtime-discovery for den versionerede
  outcome-katalog; de har ingen CLI-pendant.
- `src/mcp/tools/bookkeeping-workbench.ts` — `bookkeeping_workbench` er den
  read-only, kanoniske bankarbejdskø. Den viser den eksplicitte
  dokument-part-resolution og canonical party fra `current_document_party_links`
  (aldrig `documents.sender_name` som party-identitet), plus konto, moms og
  dimensioner. Filtre ændrer kun siden; `population.blockers` er altid for hele
  den kanoniske population. Følg række-drilldowns til bilag, party, bank,
  reviewet batch, journal og periodeluk, og brug derefter den eksisterende
  hash-bundne plan → persist → approve → apply-kontrakt for alle writes.

MCP-tools, der ikke findes på CLI'en (`src/mcp/tools/<filename>.ts` ⇒ intet
modsvar i `src/cli/`). En agent, der CLI-fortrinsstiller, vil aldrig opdage
disse uden at læse mapping-doc'en:

- `src/mcp/tools/cvr.ts` — `cvr_lookup` (CVR-opslag mod virk.dk). CLI'en har
  kun `customer cvr-lookup` (knyttet til `src/cli/customer.ts`); der findes
  ingen selvstændig `cvr`-kommando.
- `src/mcp/tools/peppol.ts` — `peppol_submit_public_invoice` (PEPPOL-submission
  af bogført faktura). CLI'en har `invoice submit-public-peppol`
  (`src/cli/invoice.ts`), men den er filtmæssigt en del af `invoice`-domænet
  — ikke en selvstændig `peppol`-CLI.
- `src/mcp/tools/efaktura.ts` — `efaktura_send` (LIVE Digisense-afsendelse af en
  udstedt offentlig e-faktura). CLI-pendanten er `invoice transmit-digisense`
  (underkommando i `src/cli/invoice/issuance.ts`), ikke en `efaktura send`-
  kommando — navnene divergerer, men begge kører samme
  `resolveDigisenseTransmitter` + `transmitPublicEInvoicePeppol`. Access-point-
  identiteten udledes deterministisk af companyKey (intet `--access-point` skal
  opfindes), så afsendelse er idempotent og leverer aldrig dobbelt.
- `src/mcp/tools/portfolio.ts` — `portfolio_overview` (workspace-vid
  porteføljeoversigt). CLI'en har kun `dashboard` (`src/cli/dashboard.ts`),
  der er virksomheds-scopet og kun delvist overlapper.
- `src/mcp/tools/period.ts` — `period_list` (lister regnskabsperioder).
  CLI'en har kun `period close`/`period reopen` (`src/cli/period.ts`) — der
  er ingen `period list`-kommando.

### CLI-only-kommandoer uden MCP-pendant

CLI-kommandoer, der ikke har et MCP-tool (`src/cli/<filename>.ts` ⇒ intet
modsvar i `src/mcp/tools/`). En agent, der MCP-fortrinsstiller, vil aldrig
opdage disse uden at læse mapping-doc'en. Genåbning af en lukket
regnskabsperiode (`period reopen`) er fx CLI-only — se også underafsnittet
"Andre kendte mikro-afvigelser" længere nede:

- `src/cli/agent.ts` — `agent run` (den pakkede runtime-agent-loop; egen
  kontrakt i `runtime-agent-contract.md`). Ingen `agent_*`-MCP-tools.
- `src/cli/annual-report.ts` — `report annual` og afledte årsrapport-flows.
  Ingen `annual_report_*`-MCP-tools.
- `src/cli/dashboard.ts` — `dashboard` (virksomheds-scopet nøgletal). MCP
  har det workspace-scopede `portfolio_overview` i stedet (se MCP-only
  ovenfor) — ikke et 1:1-modstykke.
- `src/cli/opening-balance.ts` — `opening-balance post` (etablerer
  primosaldo). Ingen `opening_balance_*`-MCP-tools.
- `src/cli/reg.ts` — `reg coverage` og `reg citations` (regulatorisk
  dækningsrapport + paragrafhenvisninger). Ingen `reg_*`-MCP-tools.
- `src/cli/compliance.ts` — `compliance report` (genererer en
  deterministisk, byte-stabil HTML-rapport til revisor eller myndighed
  med audit-kæde, backup-status, retention, GDPR, regulatorisk dækning
  og hele regel→paragraf-mappingen). Ingen `compliance_*`-MCP-tools.
- `src/cli/report.ts` — `report balance`, `report profit-loss`,
  `report trial-balance`, `report tax`. MCP har kun `tax_return_prepare`;
  resten er CLI-only.
- `src/cli/serve.ts` — `serve` (starter cockpit HTTP-API'en). Det er en
  proces-host og giver ikke mening som MCP-tool.
- `src/cli/group.ts` — strukturkommandoerne `validate-manifest`,
  `apply-manifest` og `overview` samt mellemregningskommandoerne
  `validate-mapping`, `propose-mapping`, `approve-mapping`, `revoke-mapping`
  og `reconcile`, samt balanceelimineringernes `propose-elimination`,
  `approve-elimination`, `reject-elimination`, `apply-elimination`,
  `reverse-elimination` og `eliminations`. Mappings og eliminationer har
  append-only lifecycle og særskilt reviewer;
  afstemning er read-only, eksplicit dateret og kun sammenlignelig i samme
  funktionsvaluta. Der er bevidst ingen MCP-vej, før en særskilt workspace-wide
  autorisationskontrakt er eksponeret der.
- `src/cli/workspace-access.ts` — `workspace-access bootstrap-first`:
  lokal, engangs bootstrap af workspace-ejer og medlemskaber. Den private
  sikkerhedsbootstrap må ikke udstilles som et generelt MCP-tool.
- `src/cli/workspace-snapshot.ts` — `workspace snapshot` og
  `workspace restore`: credential-fri, signeret flytning af et helt workspace
  med staged restore og eksplicit geninvitation af identiteter. Det er en
  administrativ backup-/recovery-grænse og udstilles ikke som MCP-tool.
- `src/cli/workspace-registry.ts` ↔ `src/mcp/tools/workspace-registry.ts` —
  canonical parties og immutable corporate records med explicit company-scope,
  actor/confirm på writes og filtrering før pagination.
- `src/cli/workspace-registry.ts` ↔ `src/mcp/tools/workspace-document-inbox.ts` —
  immutable workspace-indbakke med eksplicit, adgangskontrolleret routing og
  ét canonical company-handoff; kilden ligger aldrig i en workspace-hovedbog.
- `src/cli/group.ts` ↔ `src/mcp/tools/intercompany-dispositions.ts` —
  source-linked two-sided disposition lifecycle. MCP requires live, narrow
  membership on both legal-company endpoints; it can plan, propose, approve,
  link, inspect, settle and reopen evidence, but never posts either ledger.
- `src/cli/report.ts` ↔ `src/mcp/tools/cfo-analytics.ts` — versioneret,
  source-linked CFO-analyse over live journal og importarkiver. Portfolio er
  aldrig en konsolidering; group-scope delegerer kun til en godkendt profil.
- `src/cli/local.ts` — `local start`: loopback-only launcher for den simple
  én-virksomhedstilstand; det er en proces-host, ikke et MCP-tool.
- `src/cli/accounting-draft.ts` — det append-only fire-øjne-flow
  `create`/`revise`/`submit`/`reject`/`approve-and-post` samt `list`/`show`.
  Hosted HTTP håndhæver bogholder/reviewer-roller via Better Auth-medlemskab;
  CLI'en håndhæver actor-allowlist og forskellige actors, men workflowet
  udstilles ikke over MCP før samme rolle- og virksomhedsgrænse findes dér.
- `src/cli/bank-account.ts` — `bank-account add`/`bank-account list`
  (registrér/lis/opdatér bankkonti for FX-bogføring). MCP-surface'en eksponerer
  `bank_account_list` og confirm-gatede `bank_account_update`; oprettelse er
  fortsat CLI/cockpit-only.
- `src/cli/init.ts` — `init` (initialisér en virksomhed). MCP eksponerer
  `company_add` (`src/mcp/tools/system.ts`) i stedet, ikke `init` direkte.
- `src/cli/gdpr.ts` — kun slettevejen `gdpr forget` (legacy alias:
  `gdpr erase`) er CLI-only og kræver det eksplicitte flag
  `--after-retention-expiry` (exit `2` uden). `gdpr discover`, `gdpr export`
  og `gdpr audit-log` har nu direkte MCP-pendanter i `src/mcp/tools/gdpr.ts`;
  discover/export er actor- og `confirm`-gatede, mens audit-log-eksporten er
  read-only og kan signeres med en eksisterende Ed25519-backupnøgle.

**CLI-only på kommando-niveau** — filen har en MCP-tvilling, men disse
enkelt-kommandoer har intet MCP-tool (verificeret mod `rentemester --help`
og `tools/list`):

- `src/cli/company.ts` (tvilling: `src/mcp/tools/company.ts`) —
  `company list` (lister virksomheder i workspacet) og `company set-profile`
  (retter virksomhedens profil efter init) er CLI-only; MCP har kun
  `company_add`, `company_profile_get` og `company_sync_cvr`.
- `src/cli/system.ts` (tvilling: `src/mcp/tools/system.ts` +
  `src/mcp/tools/system/`) — seks kommandoer er CLI-only:
  `system export-saft` (SAF-T-eksport), `system export-public-key`,
  `system verify-backup-signature`, `system rotate-backup-keypair`,
  `system export-accountant` (håndoff-pakke til bogholder/revisor) og
  `system backup-guide` (HTML-guide).
- `src/cli/vat.ts` (tvilling: `src/mcp/tools/vat.ts`) —
  `vat momsangivelse` (alias: `vat filing`) har MCP-pendanten `vat_filing`.
  Begge bygger den read-only, hele-kroner TastSelv-form og indsender aldrig til Skattestyrelsen.
- `src/cli/import.ts` (tvilling: `src/mcp/tools/import.ts`) —
  `import run` (fuld migrering), `import systems` og `import contacts` er
  CLI-only; MCP har kun `import_archive_list`/`import_archive_year`.
- `src/cli/customer.ts` (tvilling: `src/mcp/tools/customer.ts`) —
  `customer cvr-lookup` har MCP-pendanten `cvr_lookup` (se MCP-only ovenfor);
  ingen kommandoer i filen er helt uden pendant, men navnene divergerer.

**Nye domæne-tvillinger** — disse filer har direkte, men ikke nødvendigvis
ord-for-ord, CLI/MCP-pendanter og er derfor hverken CLI-only eller MCP-only:

- `src/cli/bookkeeping-batch.ts` ↔ `src/mcp/tools/bookkeeping-batch.ts` —
  planlægning, godkendelse og anvendelse af en hash-bundet batch.
- `src/cli/supplier-commitments.ts` ↔ `src/mcp/tools/supplier-commitments.ts` —
  reviewede leverandørforpligtelser og deres deterministiske occurrence-flow.
- `src/cli/dimensions.ts` ↔ `src/mcp/tools/dimensions.ts` — company-scoped,
  append-only dimension definitions and members, including activate,
  deactivate, rename and supersede lifecycle events with queryable historical
  labels, plus hash-bound journal-line allocations. Assignments never alter
  journal bytes, VAT or legal totals.
- `src/cli/posting-rules.ts` ↔ `src/mcp/tools/posting-rules.ts` —
  forslag, lifecycle og dry-run forklaring af virksomheds-lokale
  konteringsregler.
- `src/cli/purchase-vat-preflight.ts` ↔ `src/mcp/tools/expense.ts` —
  `expense vat-preflight` og `expense_vat_preflight[_apply]`; MCP-værktøjet
  ligger under expense-domænet, mens CLI-adapteren bevidst er en selvstændig
  fil for at holde provider-I/O ude af posting-kernen.

> **Andre kendte mikro-afvigelser (samme filnavn, divergent klassifikation
> eller ergonomi):**
>
> - **`import_archive_year` har ingen selvstændig CLI-kommando.** Den henter
>   fra samme arkiv-artefakt som `import archive` skriver/lister
>   (`src/cli/import.ts`).
> - **`period reopen` er CLI-only.** En for tidligt lukket regnskabsperiode kan
>   kun genåbnes via CLI-kommandoen `period reopen` (en kontrolleret, fuldt
>   revisionssporet handling, #247) — der findes *ingen* `period_reopen`
>   MCP-tool. MCP-surface'en eksponerer `period_list` (read) og `period_close`
>   (write-irreversible), men ikke en genåbning. En agent der over MCP rammer
>   en lukket-periode-afvisning kan altså ikke selv genåbne perioden; den må
>   henvise mennesket til `rentemester period reopen`. HTTP-laget har
>   derimod `POST /api/companies/:slug/periods/reopen` — så cockpittet er
>   ikke begrænset på samme måde.
> - **`invoice create` er CLI-only — den ergonomiske CLI-vej til at fakturere.**
>   `invoice create` udsteder en kundefaktura uden at man selv skriver JSON eller
>   regner moms (linjer som `"beskrivelse|antal|stykpris"`, satsen i procent).
>   Den har *ingen* MCP-tool: MCP-pendanten er det typede `invoice_issue`, der
>   tager en fuld `InvoicePayload` med færdigberegnede totaler. En agent over MCP
>   bruger altså `invoice_issue` (typet payload) — `invoice create` er
>   CLI-ergonomien for et menneske.
> - **`invoice repair-posting` er CLI-only og administrativ.** Kommandoen
>   erstatter atomisk ét eksplicit navngivet, uklassificeret legacy-journalbilag
>   med en kanonisk fakturabogføring plus modpostering. Den afviser fakturaer med
>   betalinger, krav, kreditnotaer eller anden følge-evidens og kræver åbne
>   perioder, `--legacy-journal-entry-id`, `--reason` og actor-attribution. Den
>   eksponeres bevidst ikke som et generelt MCP-reversal-tool.
> - **`invoice export-public` og `invoice export-public-oioubl` er CLI-only.**
>   De skriver deterministiske handoff-artefakter til offentlig e-faktura
>   (henholdsvis et EAN/GLN-preview og et OIOUBL-handoff) uden PEPPOL-transport.
>   Der findes ingen `*_export_public*` MCP-tools — MCP-surface'en eksponerer kun
>   `peppol_submit_public_invoice` til den egentlige PEPPOL-submission.
> - **`portfolio_overview`** dækker delvist det CLI'en eksponerer som
>   `dashboard`, men er et workspace-tool (`workspace`-parameter, ikke
>   `company`).
> - **`customer_validate_vat` (MCP) vs. `customer validate-vat` (CLI)** —
>   begge overflader klassificerer den cache-skrivende handling som en
>   bekræftet mutation.
>
> ### Den oprindelige løse note (for historisk reference)
>
> Flere CLI-kommandoer eksponeres ikke som tools, fx `init`, `serve`,
> `report *`, `vat momsangivelse`/`vat filing`, `period reopen`,
> `gdpr erase`, `opening-balance post`, `bank-account add`/`list`,
> `import run`/`systems`/`contacts`, `agent run`, `reg coverage`/`reg citations`,
> `invoice create`, `invoice repair-posting`, `invoice export-public`/`invoice export-public-oioubl` og
> diverse `system export-*`/`verify-*`-kommandoer. Disse driver lokale
> workflows eller hører til den indbyggede `agent run`-loop og er bevidst
> holdt uden for den løse agent-surface.

### Historiske afvigelser (samlet i den nye liste ovenfor)

- **`period_list` har ingen CLI-kommando.** Tool'et `period_list` lister
  regnskabsperioder over MCP, men CLI'en har kun `period close` — der er
  ingen `period list`-kommando. (Tidligere noterede dette dokument
  `period list` som en CLI-kommando "der skal bygges"; den er aldrig blevet
  bygget. MCP-tool'et læser `accounting_periods` direkte.) Vil man genskabe
  1:1-mappingen skal en `period list`-CLI-kommando tilføjes — ellers er
  dette en bevidst, dokumenteret afvigelse.
_(De oprindelige løse notater er nu listet eksplicit pr. fil i sektionerne
"MCP-only — tools uden CLI-pendant" og "CLI-only — kommandoer uden
MCP-pendant" ovenfor.)_

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

> **SECURITY — attribution er RÅDGIVENDE, ikke et sikkerhedshegn (Audit
> 2026-06-11 SEC-4).** `createdBy` udledes af MCP-klientens egen
> `initialize`-handshake (`clientInfo.name`/`version`). En klient styrer selv de
> værdier og kan præsentere et hvilket som helst navn — også ét der står på en
> virksomheds `actor_allowlist`. Attributionen er derfor god nok som en ærlig
> audit-trail-etiket, men er IKKE et bevis på identitet. Actor-allowlisten
> (`checkActorAllowlist`, håndhævet for confirm-gatede writes i
> `tool-runtime.ts`, jf. SEC-2) er en grov politik-gate på en betroet,
> single-user/lokal transport — ikke autentifikation på en utroet
> multi-tenant-kanal. Stærk per-kalder-identitet hører hjemme i
> transport/auth-laget (jf. `src/server/auth.ts` Phase 2 / Better Auth), ikke i
> handshake-navnet.

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
