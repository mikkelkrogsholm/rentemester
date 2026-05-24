# Rentemester

## Agent-first bogholderi for danske mikrovirksomheder

**Arbejdstitel:** Rentemester  
**Koncept:** Et agent-first, headless, open source bogholdersystem for danske mikrovirksomheder.  
**Kerneidé:** Agenten er bogholderen. Ledgeren håndhæver reglerne. Skills forklarer systemet og lovgivningen.  
**Målgruppe:** Danske mikrovirksomheder, freelancere, konsulenter og små ApS’er med relativt simple forhold.

---

## 1. Elevator pitch

Rentemester er et open source, agent-first bogholdersystem for danske mikrovirksomheder.

I stedet for at brugeren manuelt klikker rundt i et traditionelt regnskabssystem, får en autonom bogholder-agent adgang til kontrollerede værktøjer via MCP og CLI. Agenten henter bilag fra email, importerer banktransaktioner, matcher bilag med banklinjer, opretter og sender fakturaer, bogfører entydige transaktioner, laver momsrapport, holder styr på exceptions og eksporterer data til revisor.

Selve bogføringen sker dog ikke frit. Rentemester har en deterministisk bogføringskerne, en append-only SQLite-ledger, versionsstyrede danske regler, audit trail, bilagshash, fakturavalidering, momslogik og eksportformater.

**Agenten handler. Reglerne afgør. Ledgeren håndhæver.**

---

## 2. Taglines

- **Agenten handler. Reglerne afgør. Ledgeren håndhæver.**
- **Bogholderen i maskinen.**
- **AI-native bogholderi med danske regler som fundament.**
- **Et åbent, agent-first bogholdersystem for danske mikrovirksomheder.**
- **The bookkeeping agent can act. The ledger must enforce.**
- **AI acts. Rules decide. Ledger enforces.**

---

## 3. Hvorfor “Rentemester”?

Rentemester er et gammelt dansk/embedsmandsagtigt ord med økonomisk tyngde. Det signalerer:

- regnskab
- forvaltning
- orden
- myndighed
- arkæisk dansk kvalitet
- noget, der passer overraskende godt til en autonom bogholder-agent

Mulige repositories/pakker:

```text
rentemester
rentemester-core
rentemester-agent
rentemester-rules-dk
rentemester-mcp
rentemester-cli
rentemester-web
rentemester-export
rentemester-invoice
```

---

## 4. Hovedidé

Traditionelle regnskabssystemer er app-first:

```text
Bruger klikker rundt
→ opretter bilag/fakturaer
→ vælger konti
→ afstemmer
→ rapporterer
```

Rentemester er agent-first:

```text
Systemet observerer
→ agenten handler
→ ledgeren validerer
→ brugeren håndterer kun exceptions
```

Brugeren arbejder ikke primært med kladder, konti og menuer. Brugeren giver systemet adgang til:

- bank CSV eller bankfeed
- dedikeret bilagsmail
- dokumentmappe
- fakturamodul
- regelbibliotek
- virksomhedens policy

Agenten gør rutinearbejdet:

- finder bilag
- læser fakturaer og kvitteringer
- matcher bilag med banktransaktioner
- klassificerer leverandører
- vælger konto og momskode
- bogfører entydige udgifter
- opretter og sender fakturaer
- matcher indbetalinger
- laver momsrapport
- laver revisor-eksport
- rejser exceptions

Ledgeren håndhæver:

- debet = kredit
- bilag påkrævet
- fakturaregler
- momsregler
- låsning af posteringer
- append-only historik
- audit trail
- hash af bilag
- backup
- eksport

---

## 5. Designfilosofi

### 5.1 Agenten må handle

Agenten skal ikke blot foreslå. Den skal være den aktive bogholder.

Den må:

- importere bankdata
- hente bilag fra email
- læse og klassificere bilag
- matche bank og bilag
- oprette posteringer
- bogføre transaktioner, der validerer
- oprette fakturaer
- sende fakturaer
- matche betalinger
- generere rapporter
- oprette exceptions

### 5.2 Ledgeren må aldrig kunne overtales

Agenten må aldrig skrive direkte i databasen. Den må kun kalde godkendte MCP-tools eller CLI-kommandoer.

Ledgeren skal afvise alt, der bryder reglerne:

- ubalanceret postering
- manglende bilag
- ugyldig momskode
- faktura uden lovpligtige oplysninger
- postering i lukket periode
- forsøg på ændring/sletning af bogført postering
- dubletfakturanummer
- manglende eller forkert dokumentation

### 5.3 Usikkerhed betyder exception

Målet er ikke “AI gætter rigtigt”. Målet er:

```text
Alt entydigt bogføres automatisk.
Alt usikkert blokeres eller sendes til exception queue.
```

Den professionelle standard er:

> Systemet skal enten bogføre korrekt eller nægte at bogføre.

### 5.4 Regler skal være åbne, versionsstyrede og testbare

Alle regler skal have:

- regel-id
- version
- officiel kilde
- gyldighedsdato
- maskinlæsbar definition
- menneskelig forklaring
- testcases
- changelog

### 5.5 Local-first og export-first

Systemet skal kunne køre lokalt eller på egen server. Data skal kunne eksporteres.

Ingen lock-in.

---

## 6. Overordnet arkitektur

```text
                   ┌──────────────────────┐
                   │ Hermes / OpenClaw     │
                   │ Bogholder-agent       │
                   └──────────┬───────────┘
                              │ MCP / CLI tool calls
                              ▼
┌────────────────────────────────────────────────┐
│ Rentemester API / MCP / CLI                    │
│                                                │
│ - bank import                                  │
│ - email ingestion                              │
│ - document extraction                          │
│ - matching                                     │
│ - invoice generation                           │
│ - posting API                                  │
│ - VAT report                                   │
│ - exports                                      │
│ - rules explain                                │
│ - system health                                │
└──────────────────┬─────────────────────────────┘
                   │ hard validation
                   ▼
┌────────────────────────────────────────────────┐
│ Deterministic core                             │
│                                                │
│ - SQLite ledger                                │
│ - append-only journal                          │
│ - rules engine                                 │
│ - VAT engine                                   │
│ - invoice validator                            │
│ - audit log                                    │
│ - document hash store                          │
│ - backup jobs                                  │
└──────────────────┬─────────────────────────────┘
                   ▼
┌────────────────────────────────────────────────┐
│ Evidence layer                                 │
│                                                │
│ - bilag                                        │
│ - emails                                       │
│ - bank CSV                                     │
│ - faktura-PDF                                  │
│ - generated documents                          │
│ - exports                                      │
└────────────────────────────────────────────────┘
```

---

## 7. De tre agent-roller

### 7.1 Runtime-bogholderen

Dette er Hermes eller OpenClaw i produktion.

Den gør det daglige arbejde:

- scanner email
- henter bilag
- læser PDF’er
- importerer bank CSV
- matcher transaktioner
- klassificerer udgifter
- bogfører entydige transaktioner
- opretter/sender fakturaer
- markerer fakturaer som betalt
- laver exceptions
- laver statusrapporter

Runtime-agenten må ikke ændre kode eller regler i produktion.

### 7.2 Builder-agenten

Dette er fx Codex eller Claude Code.

Den arbejder i repoet:

- skriver kode
- laver migrations
- bygger MCP-server
- skriver CLI-kommandoer
- opdaterer tests
- implementerer issues
- laver pull requests
- forbedrer dokumentation

Builder-agenten må ikke have adgang til produktionsledger, produktionsmail eller SMTP.

### 7.3 Rule-maintainer-agenten

En separat agent, der overvåger officielle kilder:

- Erhvervsstyrelsen
- Skattestyrelsen
- Retsinformation
- Virk
- Datatilsynet

Den må:

- opdage ændringer
- sammenligne kilder
- foreslå regelopdateringer
- generere testcases
- oprette pull requests

Den må ikke deploye regler uden review.

---

## 8. Hermes / OpenClaw som bogholder

Bogholderen er Hermes eller OpenClaw.

Rentemester er ikke agenten. Rentemester er systemet, agenten bruger.

```text
Hermes/OpenClaw
= den autonome bogholder-agent

Rentemester
= ledger, regler, database, bilag, fakturaer, audit trail, exports
```

### 8.1 Agentens ansvar

Agenten skal kunne:

- hente mails
- finde bilag
- downloade attachments
- læse PDF/HTML-fakturaer
- importere bank CSV
- matche banklinjer og bilag
- klassificere leverandører
- vælge konto
- vælge momskode
- oprette postering via API
- oprette fakturaer
- sende fakturaer
- matche betalinger
- generere momsrapport
- eksportere revisorpakke
- forklare status

### 8.2 Agentens begrænsninger

Agenten må ikke:

- skrive direkte i SQLite
- slette bilag
- ændre bogførte posteringer
- ignorere hard stops
- ændre regelversioner i produktion
- sende faktura uden issue-flow
- ændre faktura efter udstedelse
- bogføre i lukket periode

---

## 9. MCP-first og CLI-first

Rentemester bør være headless.

Agenten tilgår systemet via:

- MCP tools
- CLI commands
- eventuelt API

Dashboardet er sekundært.

### 9.1 Hvorfor MCP?

MCP giver agenten kontrollerede værktøjer i stedet for fri fil- eller databaseadgang.

Agenten kan fx kalde:

```text
bank.import_csv
documents.ingest
documents.extract
transactions.match
ledger.post_ready
invoices.create
invoices.issue
invoices.send_email
vat.generate_report
exports.accountant_package
exceptions.list
rules.explain
system.healthcheck
```

### 9.2 Hvorfor CLI?

CLI gør systemet:

- scriptbart
- testbart
- brugbart uden AI
- CI-venligt
- debugbart
- reproducerbart

Eksempel:

```bash
rentemester init
rentemester import-bank bank.csv
rentemester ingest-documents ./bilag
rentemester email scan --since 2026-05-01
rentemester match
rentemester post-ready
rentemester exceptions
rentemester invoice create --customer acme --amount 12000
rentemester invoice issue --invoice 2026-0042
rentemester invoice send --invoice 2026-0042
rentemester vat report --period 2026-Q2
rentemester export accountant --period 2026-Q2
rentemester audit verify
rentemester backup run
rentemester rules explain DK-INVOICE-FULL-004
```

---

## 10. Agent contract

Der skal være en formel kontrakt mellem agent og system.

### 10.1 Agenten må

- kalde MCP-tools
- køre CLI-kommandoer
- læse tool responses
- hente bilag fra godkendte mailkonti
- importere bank CSV
- oprette fakturakladder
- udstede fakturaer gennem invoice API
- sende fakturaer via godkendt email API
- bogføre via ledger API, hvis validering passerer
- oprette exceptions
- opsummere status

### 10.2 Agenten må ikke

- skrive direkte i SQLite
- redigere filer i dokumentarkivet uden API
- slette bilag
- ændre bogførte posteringer
- ændre udstedte fakturaer
- redigere regelversioner i produktion
- ignorere hard stops
- deploye kode
- tilgå private mails uden whitelist
- sende penge
- foretage bankbetalinger

---

## 11. Kerneflow: udgifter

```text
Bank CSV
↓
Normalisering af transaktioner
↓
Bilag hentes fra email/upload
↓
Bilag læses og metadata udtrækkes
↓
Banklinje matches med bilag
↓
Leverandør identificeres
↓
Konto og momskode bestemmes
↓
Regelmotor validerer
↓
Ledger bogfører og låser
↓
Audit trail opdateres
↓
Bilag arkiveres
```

### 11.1 Statusser

```text
ready_to_post
posted
blocked
missing_document
document_mismatch
needs_context
invalid_invoice
manual_review
reversed
```

### 11.2 Hard stops

- bilag mangler
- beløb matcher ikke bank
- faktura er ulæselig
- dansk moms kræves fratrukket, men momsbeløb mangler
- valuta mangler kurs
- leverandørland kan ikke bestemmes ved udlandsmoms
- restaurant uden formål/deltagere
- mulig privat udgift
- faktura er ikke udstedt til virksomheden, hvor det er påkrævet
- periode er lukket
- debet/kredit balancerer ikke

---

## 12. Kerneflow: fakturaer

Rentemester skal mindst kunne lave korrekte fakturaer. Gerne også sende dem.

```text
Agent/user angiver kunde, ydelse, pris, moms og betalingsbetingelser
↓
System opretter fakturakladde
↓
Faktura valideres mod regler
↓
Faktura udstedes med fortløbende nummer
↓
PDF genereres
↓
Faktura sendes via SMTP
↓
Email-log gemmes
↓
Salg bogføres
↓
Bankindbetaling matches senere
```

### 12.1 Fakturamodul skal have

- kunder
- varer/ydelser
- fakturalinjer
- moms
- rabatter
- betalingsbetingelser
- forfaldsdato
- fortløbende fakturanummer
- PDF-generering
- kreditnota
- status: draft / issued / sent / paid / overdue / credited
- email-log
- audit trail

### 12.2 Udstedte fakturaer må ikke ændres

Når en faktura er udstedt, må den ikke redigeres.

Fejl håndteres via:

- kreditnota
- ny faktura
- tydelig audit trail

### 12.3 SMTP-log

Ved fakturaudsendelse skal systemet gemme:

- faktura-id
- modtager
- cc/bcc hvis brugt
- tidspunkt
- subject
- message-id
- attachment hash
- SMTP-resultat
- eventuelle fejl

---

## 13. Email-bilag

Systemet skal kunne hente bilag fra en dedikeret mailboks.

Eksempler:

```text
bilag@firma.dk
invoice@firma.dk
receipts@firma.dk
```

### 13.1 Email ingestion skal kunne

- forbinde via IMAP, Gmail API eller Microsoft Graph
- søge efter nye mails
- downloade attachments
- gemme original email
- gemme headers
- gemme attachments
- udtrække PDF/HTML-fakturaer
- markere mail som behandlet
- undgå dubletter
- linke mail til dokument og postering

### 13.2 Original mail skal gemmes

Mailen kan være en del af dokumentationen. Gem derfor:

- raw `.eml`
- headers
- body
- attachments
- message-id
- from/to/date/subject
- hash

### 13.3 Sikkerhed ved email

Email-adgang er følsomt. Systemet skal bruge:

- separat bilagsmail
- OAuth hvor muligt
- krypteret token storage
- least privilege
- audit log
- databehandleraftaler hvor relevant

---

## 14. Bilagsarkiv

Bilag er beviser, ikke bare filer.

Hvert bilag skal registreres i databasen med metadata.

### 14.1 Dokumentfelter

```text
document_id
document_no
source
original_filename
stored_path
mime_type
sha256_hash
upload_datetime
email_message_id
supplier_name
supplier_cvr_or_vat
supplier_country
invoice_no
invoice_date
due_date
amount_ex_vat
vat_amount
amount_inc_vat
currency
ocr_text
extraction_json
status
linked_bank_transaction_id
linked_journal_entry_id
```

### 14.2 Versionering

Hvis et bilag erstattes, må det gamle ikke slettes.

Brug:

```text
document_versions
```

med:

- version number
- previous hash
- new hash
- reason
- created_at
- created_by

---

## 15. Bogføringskerne

Kernen skal være deterministisk.

### 15.1 Minimumstabeller

```text
companies
users
fiscal_years
periods
accounts
standard_accounts
vat_codes
parties
vendors
customers
documents
document_versions
bank_accounts
bank_transactions
reconciliations
invoices
invoice_lines
invoice_events
journal_entries
journal_lines
ledger_hashes
audit_log
agent_decisions
exceptions
email_messages
email_attachments
system_settings
backups
legal_sources
rules
```

### 15.2 Journal entries

```sql
CREATE TABLE journal_entries (
  id INTEGER PRIMARY KEY,
  entry_no TEXT NOT NULL UNIQUE,
  transaction_date TEXT NOT NULL,
  registration_datetime TEXT NOT NULL,
  text TEXT NOT NULL,
  source_bank_transaction_id INTEGER,
  document_id INTEGER,
  invoice_id INTEGER,
  rule_version TEXT NOT NULL,
  created_by TEXT NOT NULL,
  created_by_program TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('posted','reversed')),
  reversal_of_entry_id INTEGER,
  previous_hash TEXT,
  entry_hash TEXT NOT NULL,
  locked INTEGER NOT NULL DEFAULT 1
);
```

### 15.3 Journal lines

```sql
CREATE TABLE journal_lines (
  id INTEGER PRIMARY KEY,
  journal_entry_id INTEGER NOT NULL,
  account_id INTEGER NOT NULL,
  debit_amount NUMERIC NOT NULL DEFAULT 0,
  credit_amount NUMERIC NOT NULL DEFAULT 0,
  vat_code_id INTEGER,
  currency TEXT NOT NULL DEFAULT 'DKK',
  exchange_rate NUMERIC,
  text TEXT,
  FOREIGN KEY(journal_entry_id) REFERENCES journal_entries(id)
);
```

### 15.4 Grundregler

- Debet skal være lig kredit.
- Bogførte posteringer må ikke ændres.
- Bogførte posteringer må ikke slettes.
- Fejl rettes med nye posteringer.
- Posteringer får fortløbende ID.
- Posteringer får registreringsdato.
- Posteringer angiver hvem/hvad der bogførte.
- Bilag skal være linket, hvor relevant.
- Regelversion skal gemmes.
- Periode skal være åben.
- Audit log skal opdateres.

---

## 16. Hash chain

For at gøre historikken manipulationsresistent kan hver postering hashes.

```text
entry_hash = sha256(canonical_entry_data + previous_entry_hash)
```

Fordele:

- ændringer kan opdages
- audit bliver stærkere
- exports kan verificeres
- systemet bliver mere troværdigt

---

## 17. Kontoplan

Rentemester bør tage udgangspunkt i den danske fællesoffentlige standardkontoplan, men bruge en slank intern kontoplan.

### 17.1 To lag

```text
1. Intern kontoplan
   Den brugeren/agenten bogfører på.

2. Mapping til officiel standardkontoplan
   Bruges til SAF-T, årsrapport, exports og revisor.
```

### 17.2 Eksempel på interne konti

```text
1000 Omsætning, ydelser
1200 Salgsmoms
2000 Bank
3000 Software og SaaS
3010 AI-værktøjer
3020 Hosting og cloud
3030 Telefon og internet
3040 Kontorartikler
3050 Rejse og transport
3060 Hotel og ophold
3070 Repræsentation
3080 Abonnementer
3090 Forsikring
3100 Revisor og rådgivning
3110 Marketing
3120 Hardware og udstyr
4000 Købsmoms
4010 EU-erhvervelsesmoms
4020 Moms af ydelser fra udlandet
4500 Momsafregning
5000 Egenkapital
5100 Mellemregning ejer
```

De præcise kontonumre bør fastlægges i projektet og mappes til standardkontoplanen.

### 17.3 Account schema

```sql
CREATE TABLE accounts (
  id INTEGER PRIMARY KEY,
  account_no TEXT NOT NULL UNIQUE,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  normal_balance TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1,
  standard_account_no TEXT,
  standard_account_name TEXT,
  standard_account_version TEXT,
  annual_report_line TEXT,
  ixbrl_concept TEXT,
  default_vat_code_id INTEGER,
  allow_direct_posting INTEGER NOT NULL DEFAULT 1
);
```

---

## 18. Moms

Moms må ikke hardcodes som “25% på alt”.

Momsmotoren skal kunne håndtere:

- dansk køb med dansk moms
- dansk salg med dansk moms
- køb uden moms
- EU-køb med reverse charge
- køb uden for EU
- EU-salg
- eksport
- ikke-fradragsberettigede udgifter
- delvist fradrag
- repræsentation
- hotel/restaurant/særlige cases

### 18.1 Momsrapport

Systemet skal kunne lave:

```text
salgsmoms
købsmoms
EU-erhvervelsesmoms
reverse charge
rubrik A/B/C hvis relevant
skyldig/tilgodehavende moms
momsafstemning mod momskonti
```

### 18.2 VAT code schema

```text
vat_code_id
name
type
rate
country_scope
purchase_or_sale
reverse_charge
deductible_percentage
posting_accounts
valid_from
valid_to
rule_id
```

### 18.3 Eksempler på momskoder

```text
DK_PURCHASE_25
DK_SALE_25
NO_VAT
EU_SERVICE_REVERSE_CHARGE
NON_EU_SERVICE_REVERSE_CHARGE
NON_DEDUCTIBLE
PARTIAL_DEDUCTIBLE
REPRESENTATION_SPECIAL
```

---

## 19. Fakturaregler

Fakturaer skal valideres før udstedelse.

### 19.1 Fuld faktura skal typisk indeholde

- fakturadato
- fortløbende fakturanummer
- sælgers navn og adresse
- sælgers CVR/SE/VAT
- købers navn og adresse
- beskrivelse af vare/ydelse
- mængde/omfang
- leveringsdato hvis relevant
- enhedspris uden moms
- rabatter hvis relevant
- momsgrundlag
- momssats
- momsbeløb
- totalbeløb
- betalingsbetingelser

### 19.2 Fakturastatusser

```text
draft
validated
issued
sent
paid
overdue
credited
cancelled_draft
```

### 19.3 Kreditnota

En udstedt faktura må ikke bare ændres eller slettes.

Fejl håndteres med:

- kreditnota
- ny faktura
- link mellem dokumenterne
- audit trail

---

## 20. E-faktura

V1 kan starte med PDF via SMTP, men datamodellen bør være klar til e-faktura.

Fremtidige formater:

- OIOUBL
- Peppol BIS
- NemHandel via gateway

Anbefaling:

```text
Byg ikke Peppol/NemHandel transport selv i v1.
Brug gateway senere.
```

Men invoice model skal kunne eksportere strukturerede data.

---

## 21. Bankimport og afstemning

### 21.1 Bank CSV import

Systemet skal kunne importere:

- dato
- tekst
- beløb
- valuta
- konto/kort
- reference
- saldo hvis tilgængelig

### 21.2 Normalisering

Alle banklinjer normaliseres til:

```text
bank_transaction_id
account_id
transaction_date
booking_date
text
amount
currency
counterparty_guess
reference
source_file_hash
import_batch_id
status
```

### 21.3 Matching

Match bank ↔ bilag på:

- beløb
- dato ± tolerance
- leverandørnavn
- valuta
- reference
- korttekst
- fakturanummer

### 21.4 Debitorbetaling

Når systemet sender fakturaer, skal bankimport også kunne matche indbetalinger med åbne fakturaer.

---

## 22. Exceptions

Autonom bogholder betyder ikke ingen problemer. Det betyder, at alt sikkert klares automatisk, og resten samles i exception queue.

### 22.1 Exception-typer

```text
missing_document
document_amount_mismatch
unknown_vendor
invalid_invoice
unclear_vat
missing_business_purpose
possible_private_expense
foreign_invoice_unclear
email_ingestion_failed
smtp_failed
backup_failed
rule_update_available
bank_duplicate
invoice_overdue
```

### 22.2 Exception schema

```text
exception_id
type
severity
status
related_bank_transaction_id
related_document_id
related_invoice_id
rule_id
message
required_action
created_at
resolved_at
resolved_by
resolution_note
```

---

## 23. Dashboard

Dashboardet er kontrolrummet, ikke hovedarbejdsfladen.

Det skal vise:

- autobogført denne periode
- exceptions
- manglende bilag
- unmatched banklinjer
- åbne fakturaer
- forfaldne fakturaer
- betalte fakturaer
- forventet moms
- seneste backup
- system health
- regelversioner
- seneste agenthandlinger

Eksempel:

```text
Godmorgen.

Jeg har:
- fundet 4 nye bilag i mailen
- importeret 7 banktransaktioner
- matchet 6 af dem
- bogført 5 automatisk
- oprettet 1 exception
- markeret 1 faktura som betalt
- opdateret momsestimatet

Exception:
Restaurantbilag 842 kr. mangler formål og deltagere.
```

---

## 24. Skills

Skills forklarer agenten, hvordan systemet bruges, og hvordan danske bogføringsregler skal forstås.

### 24.1 System-skills

```text
skills/system/agent_contract.skill.md
skills/system/bank_import.skill.md
skills/system/email_receipt_ingestion.skill.md
skills/system/document_ingestion.skill.md
skills/system/posting_flow.skill.md
skills/system/invoice_flow.skill.md
skills/system/exception_handling.skill.md
skills/system/audit_and_backup.skill.md
skills/system/export_flow.skill.md
```

### 24.2 Regel-skills

```text
skills/rules/dk_bookkeeping_locks.skill.md
skills/rules/dk_full_invoice.skill.md
skills/rules/dk_purchase_vat.skill.md
skills/rules/dk_reverse_charge.skill.md
skills/rules/dk_credit_note.skill.md
skills/rules/dk_representation.skill.md
skills/rules/dk_saf_t.skill.md
skills/rules/dk_email_evidence.skill.md
```

### 24.3 Eksempel på system-skill

```markdown
# Skill: Posting Ready Transactions

Use this skill when posting matched transactions.

Never write directly to SQLite.

Steps:
1. Run `rentemester transactions ready`
2. For each ready transaction, run `rentemester ledger validate --transaction <id>`
3. If valid, run `rentemester ledger post --transaction <id>`
4. If rejected, create or update an exception.
5. Summarize posted and rejected items.

Hard rule:
Do not override validation failures.
```

### 24.4 Eksempel på regel-skill

```markdown
# Skill: Danish Full Invoice Validation

Use this skill when validating a sales or purchase invoice.

A full invoice must include:
- invoice date
- sequential invoice number
- seller name/address
- seller CVR/SE/VAT
- buyer name/address
- description of goods/services
- VAT base
- VAT rate
- VAT amount
- total amount

If Danish VAT is claimed and VAT amount is not stated separately:
- reject VAT deduction
- create exception
- do not auto-post as deductible VAT

Use rule IDs:
- DK-INVOICE-FULL-001
- DK-INVOICE-FULL-002
```

---

## 25. Machine-readable rules

Skills forklarer. YAML-regler håndhæver.

Eksempel:

```yaml
rule_id: DK-INVOICE-FULL-004
name: Danish VAT must be stated separately
category: invoice_validation
source:
  authority: Skattestyrelsen
  title: Fakturaens indhold
  url: "TODO"
condition:
  invoice.country: DK
  invoice.vat_claimed: true
require:
  - invoice.vat_rate
  - invoice.vat_amount
on_fail:
  action: reject_vat_deduction
  severity: hard_stop
explanation: >
  When Danish VAT is claimed, the invoice must state VAT rate and VAT amount separately.
```

---

## 26. Golden tests

Regler skal testes med konkrete eksempler.

Eksempel:

```yaml
case: danish_invoice_missing_vat_amount
input:
  invoice:
    country: DK
    total: 1250
    text: "inkl. moms"
    vat_amount: null
expected:
  status: rejected
  reason: DK-INVOICE-FULL-004
```

Eksempel:

```yaml
case: restaurant_without_business_purpose
input:
  bank_amount: 900.00
  receipt:
    category: restaurant
    vat_amount: 180.00
    purpose: null
expected:
  status: blocked
  reason: missing_business_purpose
```

---

## 27. Lov- og kildeliste

Dette er regelbibliotekets startpunkt. Alle kilder skal gemmes med download-dato, URL, hash og versionsnotat.

### 27.1 Bogføring

- Bogføringsloven
- Vejledning om bogføringsloven
- Erhvervsstyrelsens sider om digital bogføring
- Ikke-registrerede digitale bogføringssystemer
- Bekendtgørelse om krav til digitale bogføringssystemer
- Tidsplan for digital bogføring
- Krav til procedurebeskrivelse

### 27.2 Standardkontoplan og SAF-T

- Fællesoffentlig standardkontoplan
- SAF-T-specifikationer
- Versioner/ændringer af standardkontoplanen
- Regnskab Basis upload af regnskabsfil
- SAF-T 1.0/2.0 dokumentation

### 27.3 Moms

- Momsloven
- Momsbekendtgørelsen
- Skattestyrelsens juridiske vejledning om fakturaer
- Fuld faktura
- Forenklet faktura og kassebon
- Dokumentation for købsmoms
- Momsregistrering
- Momsfrister
- EU-køb
- EU-salg
- Køb uden for EU
- Reverse charge
- Ikke-fradragsberettigede udgifter
- Repræsentation og særlige fradragsregler

### 27.4 Årsrapport og iXBRL

- Årsregnskabsloven
- Erhvervsstyrelsens vejledning om årsrapporter
- Regnskab Basis
- Regnskab Special
- Teknisk vejledning og kontroller for Regnskab Indberet
- ÅRL-taksonomi
- iXBRL-krav

### 27.5 Skat

- Skattekontrolloven
- Oplysningsskema for selskaber
- Oplysningsskema for personligt ejede virksomheder
- Frister for oplysningsskema
- Skat af egen virksomhed

### 27.6 E-faktura

- OIOUBL
- Peppol BIS
- NemHandel
- Virksomhedsguidens e-faktura-vejledninger
- Krav til offentlige kunder/EAN/GLN

### 27.7 GDPR og sikkerhed

- Databeskyttelsesloven
- GDPR
- Datatilsynets GDPR-univers for små virksomheder
- Dataansvarlig/databehandler
- Behandlingssikkerhed
- Privacy by design
- Databehandleraftaler

---

## 28. Legal sources schema

```sql
CREATE TABLE legal_sources (
  id INTEGER PRIMARY KEY,
  title TEXT NOT NULL,
  authority TEXT NOT NULL,
  url TEXT NOT NULL,
  category TEXT NOT NULL,
  downloaded_at TEXT NOT NULL,
  effective_from TEXT,
  effective_to TEXT,
  document_hash TEXT NOT NULL,
  local_path TEXT NOT NULL,
  version_label TEXT,
  notes TEXT
);
```

Regler peger tilbage til kilder:

```sql
CREATE TABLE rules (
  id INTEGER PRIMARY KEY,
  rule_id TEXT NOT NULL UNIQUE,
  rule_name TEXT NOT NULL,
  rule_version TEXT NOT NULL,
  source_id INTEGER NOT NULL,
  source_section TEXT,
  machine_rule TEXT NOT NULL,
  human_explanation TEXT NOT NULL,
  last_reviewed_at TEXT NOT NULL,
  FOREIGN KEY(source_id) REFERENCES legal_sources(id)
);
```

---

## 29. Repo-struktur

```text
rentemester/
  README.md
  LICENSE
  SECURITY.md
  CONTRIBUTING.md

  docs/
    vision.md
    architecture.md
    agent_contract.md
    compliance_model.md
    threat_model.md
    accounting_principles.md
    mcp_tools.md
    cli_reference.md
    rule_governance.md
    data_model.md
    invoice_model.md
    vat_model.md
    backup_model.md

  cli/
    rentemester

  mcp-server/
    server.py
    tools/
      bank.py
      email.py
      documents.py
      ledger.py
      invoices.py
      vat.py
      rules.py
      exports.py
      system.py
      exceptions.py

  core/
    db/
      schema.sql
      migrations/
    ledger/
      posting.py
      validation.py
      audit.py
      hashing.py
      periods.py
    invoices/
      model.py
      pdf.py
      email.py
      validation.py
      credit_note.py
    documents/
      storage.py
      extraction.py
      matching.py
      email_ingestion.py
    bank/
      import_csv.py
      normalize.py
      reconcile.py
    vat/
      engine.py
      report.py
    rules/
      engine.py
      loader.py
      sources.py
    exports/
      accountant.py
      csv.py
      excel.py
      saf_t.py
    backup/
      snapshot.py
      remote.py
      restore_test.py

  rules/
    dk/
      bookkeeping.yaml
      invoices.yaml
      vat.yaml
      accounts.yaml
      hard_stops.yaml
      saf_t.yaml
      annual_report.yaml
      gdpr.yaml
      sources.yaml

  skills/
    system/
      agent_contract.skill.md
      bank_import.skill.md
      email_receipt_ingestion.skill.md
      document_ingestion.skill.md
      posting_flow.skill.md
      invoice_flow.skill.md
      exception_handling.skill.md
      audit_and_backup.skill.md
      export_flow.skill.md
    rules/
      dk_bookkeeping_locks.skill.md
      dk_full_invoice.skill.md
      dk_purchase_vat.skill.md
      dk_reverse_charge.skill.md
      dk_credit_note.skill.md
      dk_representation.skill.md
      dk_saf_t.skill.md

  app/
    api/
    dashboard/
    templates/
    static/

  tests/
    golden/
      invoices/
      vat/
      ledger/
      bank_matching/
      email_ingestion/
    unit/
    integration/

  examples/
    sample_company/
    sample_bank_csv/
    sample_receipts/
    sample_invoices/

  scripts/
    import_standard_chart.py
    update_legal_sources.py
    validate_rules.py
    create_demo_company.py
```

---

## 30. Folderstruktur for en virksomhed

```text
/company-bookkeeping/
  data/
    ledger.sqlite
    ledger.sqlite-wal

  bank/
    incoming_csv/
    processed/

  documents/
    inbox/
    email/
    purchases/
      2026/
    sales/
      2026/
    originals/
    archived/

  invoices/
    drafts/
    issued/
    sent/
    pdf/
    oio-ubl/
    peppol/

  exports/
    vat/
    accountant/
    audit/
    saf-t/
    annual-report/

  backups/
    local-staging/

  rules/
    active/
    archived/

  logs/
    audit.log
    email.log
    agent.log
    backup.log
```

---

## 31. MCP tools

### 31.1 Bank

```text
bank.import_csv
bank.list_transactions
bank.normalize
bank.detect_duplicates
bank.reconcile
```

### 31.2 Email

```text
email.scan_receipts
email.fetch_message
email.download_attachments
email.mark_processed
email.list_unprocessed
```

### 31.3 Documents

```text
documents.ingest
documents.extract
documents.validate_invoice
documents.match_bank_transaction
documents.archive
documents.get
```

### 31.4 Ledger

```text
ledger.validate_posting
ledger.post_expense
ledger.post_ready
ledger.reverse_entry
ledger.get_entry
ledger.list_entries
ledger.audit_verify
```

### 31.5 Invoices

```text
invoices.create_draft
invoices.validate
invoices.issue
invoices.render_pdf
invoices.send_email
invoices.create_credit_note
invoices.mark_paid
invoices.list_open
```

### 31.6 VAT

```text
vat.classify
vat.calculate
vat.generate_report
vat.reconcile
vat.export
```

### 31.7 Rules

```text
rules.explain
rules.list
rules.get
rules.validate
rules.source
rules.version
```

### 31.8 Exceptions

```text
exceptions.list
exceptions.get
exceptions.resolve
exceptions.create
exceptions.update
```

### 31.9 Exports

```text
exports.accountant_package
exports.csv
exports.excel
exports.saf_t
exports.audit_package
```

### 31.10 System

```text
system.healthcheck
system.backup_run
system.backup_status
system.restore_test
system.version
system.rule_versions
```

---

## 32. MCP response example

```json
{
  "tool": "ledger.post_ready",
  "result": {
    "posted": [
      {
        "entry_no": "2026-0042",
        "bank_transaction_id": "bank_184",
        "document_id": "doc_991",
        "account": "3620 Software og SaaS",
        "vat_code": "EU_SERVICE_REVERSE_CHARGE",
        "rule_version": "dk-vat-2026.05.16"
      }
    ],
    "rejected": [
      {
        "bank_transaction_id": "bank_185",
        "reason": "missing_business_purpose",
        "rule_id": "DK-REPRESENTATION-002",
        "exception_id": "ex_77"
      }
    ]
  }
}
```

---

## 33. Backup og drift

Hvis Rentemester bliver source of truth, backup er en kernefunktion.

Praktiske, kildehenviste opsætningsguides (BEK 205/2024 § 4, stk. 2)
ligger i [docs/compliance/backup-destinations.md](docs/compliance/backup-destinations.md),
med konkrete udbyder-guides (fx Google Workspace) i
[docs/compliance/backup-destinations/](docs/compliance/backup-destinations/).

### 33.1 Backup-krav

Systemet skal kunne:

- lave daglig lokal snapshot
- lave mindst ugentlig fuld backup
- kryptere backups
- sende backup til EU/EØS tredjepart
- logge backup-resultat
- alarmere ved fejl
- køre restore-test periodisk
- dokumentere backup-status

### 33.2 Backup-status i dashboard

```text
Seneste backup: OK
Seneste fulde backup: 2026-05-16 02:00
Seneste restore-test: OK
Backup destination: EU/EØS
Integritetscheck: OK
```

---

## 34. Sikkerhed

### 34.1 Roller

```text
admin
agent
accountant_readonly
viewer
system
```

### 34.2 Secrets

Følgende skal krypteres:

- mail OAuth tokens
- SMTP credentials
- API keys
- backup credentials
- AI provider keys

### 34.3 Least privilege

Agenten skal kun have adgang til:

- dedikeret bilagsmail
- godkendte mapper
- MCP tools
- nødvendige secrets

Ikke til:

- hele private mailkonto
- bankbetalinger
- direkte databasewrite
- regel-deploy

---

## 35. GDPR

Rentemester behandler potentielt personoplysninger.

Systemet skal derfor understøtte:

- behandlingsoversigt
- adgangsstyring
- databehandleraftaler
- logning
- sletning efter regler/policy
- eksport af data
- dokumentation for behandlingssikkerhed
- privacy by design

Email-ingestion gør GDPR ekstra vigtig, fordi mails kan indeholde oplysninger uden for regnskabets scope.

Anbefaling:

```text
Brug dedikeret bilagsmail.
Undgå adgang til hele private mailkonti.
```

---

## 36. Årsregnskab og iXBRL

Dette bør ikke være v1, men kan være roadmap.

### 36.1 Krævede moduler

```text
trial_balance_skill
closing_entries_skill
annual_report_mapping_skill
disclosure_notes_skill
ixbrl_generation_skill
ixbrl_validation_skill
filing_package_skill
```

### 36.2 Årsafslutning kræver

- saldobalance
- resultatopgørelse
- balance
- momsafstemning
- bankafstemning
- periodiseringer
- afskrivninger
- skattemæssige reguleringer
- egenkapital
- noter
- anvendt regnskabspraksis
- ledelsespåtegning hvis relevant
- revision/fravalg hvis relevant

### 36.3 iXBRL

På sigt:

- mapping fra kontoplan til årsrapportlinjer
- mapping til taksonomi
- inline XBRL-generering
- validering mod tekniske kontroller
- indberetningspakke

---

## 37. Eksport

Minimum:

- CSV
- Excel
- PDF-pakke
- bilagsarkiv
- saldobalance
- momsrapport
- debitorliste
- kreditorliste
- journal
- audit log

Senere:

- SAF-T
- iXBRL
- revisorpakke
- Regnskab Basis/Special-klargøring

---

## 38. Open source governance

Rentemester skal have stærk governance, især omkring regler.

### 38.1 Pull requests til regler skal kræve

- officiel kilde
- regel-id
- forklaring
- machine-readable rule
- golden test
- changelog
- review

### 38.2 Ingen regel uden test

En regelændring uden test må ikke merges.

### 38.3 Security policy

Projektet skal have:

- SECURITY.md
- responsible disclosure
- secret handling guidelines
- threat model
- release signing på sigt

### 38.4 Disclaimer

Rentemester skal tydeligt sige:

- projektet er ikke revisor
- projektet er ikke juridisk rådgivning
- brugeren er ansvarlig for bogføringen
- regler kan være ufuldstændige
- brug med revisor ved årsafslutning

---

## 39. MVP v1

### 39.1 V1 scope

```text
- dansk mikrovirksomhed/lille ApS
- bank CSV import
- email-bilagsimport
- manuel bilagsupload
- dokumentarkiv med hash
- AI-læsning af bilag
- bank/bilag matching
- dansk kontoplan-lite
- momsmotor
- autonom udgiftsbogføring
- fakturaoprettelse
- PDF-faktura via SMTP
- kreditnota
- debitorbetaling via bankmatch
- dashboard
- exception queue
- audit log
- backup
- eksport til revisor
- MCP-server
- CLI
```

### 39.2 Ikke v1

```text
- løn
- lager
- POS/kasse
- kompleks import/told
- avanceret projektregnskab
- fuld årsrapport/iXBRL
- multi-country
- fuld Peppol/NemHandel uden gateway
- bankbetalinger
```

---

## 40. Roadmap

### v0.1 — Vision og regelbibliotek

- repo
- README
- arkitektur
- agent contract
- første kildeliste
- dansk kontoplan-lite
- hard stops
- golden test format

### v0.2 — Ledger core

- SQLite schema
- journal entries
- journal lines
- append-only model
- audit log
- hash chain
- perioder
- kontoplan

### v0.3 — Bank og dokumenter

- bank CSV import
- dokumentarkiv
- bilagsupload
- hash
- basic extraction
- matching

### v0.4 — Moms og udgifter

- momskoder
- VAT engine
- udgiftsbogføring
- hard stops
- exception queue
- golden tests

### v0.5 — MCP og CLI

- MCP-server
- CLI commands
- agent contract enforcement
- structured tool responses

### v0.6 — Faktura

- kunder
- fakturakladder
- fakturavalidering
- PDF-generering
- issue-flow
- kreditnota

### v0.7 — SMTP og debitorer

- send faktura via SMTP
- email log
- åbne fakturaer
- bankmatch af indbetalinger

### v0.8 — Dashboard

- status
- exceptions
- posteringer
- bilag
- fakturaer
- momsestimat
- system health

### v0.9 — Revisorpakke og backup

- Excel/CSV exports
- bilagspakke
- momsrapport
- saldobalance
- backup jobs
- restore-test

### v1.0 — Source-of-truth ready

- compliance checklist
- procedurebeskrivelse
- audit verify
- stable CLI/MCP
- dokumentation
- first production-ready release

### v1.1+

- SAF-T
- OIOUBL/Peppol via gateway
- iXBRL årsrapport
- multi-company
- rule update agent

---

## 41. Eksempel på daglig agentkørsel

```text
1. Scan bilagsmail
2. Importér nye attachments
3. Hash og arkivér bilag
4. Ekstrahér fakturadata
5. Importér bank CSV
6. Match bilag mod banklinjer
7. Klassificér entydige udgifter
8. Valider moms og konto
9. Bogfør alt klar-til-bogføring
10. Opret exceptions for resten
11. Match indbetalinger mod fakturaer
12. Opdater momsrapport
13. Kør backup-check
14. Send status til bruger
```

Eksempelstatus:

```text
Rentemester status

Bogført automatisk:
- 12 udgifter
- 2 fakturabetalinger

Nye bilag:
- 15 fundet i email
- 14 matchet
- 1 unmatched

Exceptions:
- Restaurantbilag mangler formål
- Apple-bilag kræver klassifikation

Fakturaer:
- 1 faktura sendt
- 2 åbne
- 0 forfaldne

Moms:
- Foreløbig skyldig moms: 8.420 kr.

System:
- Backup OK
- Ledger integrity OK
- Regelversion: dk-2026.05.16
```

---

## 42. Eksempel på policy

```yaml
company_policy:
  company_name: "Example ApS"
  country: "DK"
  currency: "DKK"

  private_expenses_allowed: false

  documents:
    require_document_for_every_bank_transaction: true
    store_original_email: true
    hash_documents: true

  posting:
    auto_post_known_vendors: true
    auto_post_new_vendors_if_rules_are_deterministic: true
    block_if_uncertain: true
    allow_direct_sql_write: false

  invoice:
    allow_pdf_email: true
    require_validation_before_issue: true
    lock_issued_invoices: true
    use_credit_note_for_corrections: true

  blocked_categories:
    - private_expense
    - restaurant_without_business_purpose
    - hotel_without_trip_context
    - missing_document
    - invalid_invoice
    - unclear_vat
```

---

## 43. Eksempel på vendor rules

```yaml
vendors:
  OPENAI:
    match_patterns:
      - "OPENAI"
      - "CHATGPT"
    account: "3010 AI-værktøjer"
    vat_rule: "read_invoice_foreign_service"
    auto_post: true
    require_document: true

  GOOGLE:
    match_patterns:
      - "GOOGLE"
      - "GOOGLE CLOUD"
      - "GOOGLE WORKSPACE"
    account: "3020 Hosting og cloud"
    vat_rule: "read_invoice"
    auto_post: true
    require_document: true

  DSB:
    match_patterns:
      - "DSB"
    account: "3050 Rejse og transport"
    vat_rule: "danish_vat_if_invoice_shows_vat"
    auto_post: true
    require_document: true

  RESTAURANT:
    match_patterns:
      - "RESTAURANT"
      - "CAFE"
      - "BAR"
    account: "3070 Repræsentation"
    vat_rule: "special_case"
    auto_post: false
    require_business_purpose: true
```

---

## 44. Eksempel på hard stops

```yaml
hard_stops:
  - id: missing_document
    message: "No document is linked to the bank transaction."

  - id: amount_mismatch
    message: "Document amount does not match bank amount."

  - id: invalid_invoice_missing_vat_amount
    message: "Danish VAT cannot be deducted because VAT amount is missing."

  - id: restaurant_missing_business_purpose
    message: "Restaurant expense requires business purpose and participants."

  - id: closed_period
    message: "Cannot post to a closed period."

  - id: debit_credit_unbalanced
    message: "Debit and credit do not balance."

  - id: issued_invoice_modification
    message: "Issued invoices cannot be modified. Use credit note."
```

---

## 45. Eksempel på CLI-session

```bash
rentemester email scan --mailbox bilag@firma.dk --since 2026-05-01
rentemester import-bank ./bank/may.csv
rentemester documents extract --new
rentemester match
rentemester transactions ready
rentemester post-ready
rentemester exceptions list
rentemester vat report --period 2026-Q2
rentemester export accountant --period 2026-Q2
```

Mulig output:

```text
Email scan:
- 9 messages scanned
- 6 attachments imported
- 0 duplicates

Bank import:
- 14 transactions imported
- 2 duplicates skipped

Matching:
- 11 matched
- 3 unmatched

Posting:
- 9 posted
- 2 rejected

Exceptions:
- EX-104: restaurant_missing_business_purpose
- EX-105: document_amount_mismatch
```

---

## 46. Hvad gør projektet særligt?

Rentemester er ikke bare endnu et regnskabsprogram.

Det er:

- agent-first
- headless
- open source
- dansk-regelbaseret
- MCP/CLI-native
- local-first
- evidence-first
- audit-first
- exception-first
- export-first

Det er designet til en fremtid, hvor AI-agenter faktisk udfører arbejdet, men hvor kritiske systemer stadig skal være deterministiske og reviderbare.

---

## 47. Første GitHub README-udkast

```markdown
# Rentemester

Rentemester is an agent-first bookkeeping system for Danish micro-businesses.

It combines an autonomous bookkeeping agent with a deterministic double-entry ledger, versioned Danish accounting rules, document evidence, VAT validation, invoice generation, audit trail, and export-ready data.

AI acts. Rules decide. Ledger enforces.

## Core ideas

- Headless by default
- MCP and CLI first
- SQLite append-only ledger
- Danish rule library
- Document and email evidence
- Autonomous bookkeeping via Hermes/OpenClaw
- Hard validation in the ledger
- Exception queue for uncertainty
- Open source rule governance

## Status

Experimental. Not legal, tax, or accounting advice.

## Target users

- Danish freelancers
- small ApS companies
- consultants
- micro-businesses with simple bookkeeping
- company card expenses
- PDF invoices and receipts
- basic sales invoices

## Non-goals for v1

- payroll
- inventory
- POS/cash register
- complex import/toll
- full annual report/iXBRL
- bank payments
```

---

## 48. Kritisk produktbeslutning

Rentemester skal ikke starte som “fuld Dinero”.

Det skal starte som:

> En agent-first udgifts- og fakturabogholder for simple danske mikrovirksomheder.

Først når udgifter, bilag, fakturaer, moms, audit, backup og exports fungerer stabilt, bør projektet udvides med SAF-T, e-faktura, årsregnskab og iXBRL.

---

## 49. Den endelige vision

Rentemester er en ny kategori:

```text
Agent-first bogholderi
```

Ikke et regnskabssystem med AI.

Et bogholdersystem bygget til AI-agenter fra første linje kode.

```text
Agenten er bogholderen.
Ledgeren er loven.
Reglerne er kontrakten.
Bilagene er beviserne.
Dashboardet er kontrolrummet.
```

