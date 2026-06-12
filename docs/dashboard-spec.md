# Dashboard spec

Statisk HTML-dashboard for Rentemester. Genereret deterministisk fra
kerne-API'er. Bruger `DESIGN.md`-tokens. Ingen JavaScript-framework, ingen
server, ingen frontend-stack.

Outputtet er et enkelt `dashboard.html`-dokument som CLI'en kan skrive til
disk, sende som vedhæftet fil eller uploade til en bucket. Filen skal kunne
åbnes direkte i en browser uden netværk og uden eksterne assets ud over de
fonte som `DESIGN.md` allerede angiver.

## Mål

Dashboardet svarer på spørgsmålet: "Er min bogføring i orden lige nu, og hvad
skal jeg handle på?". Det er ikke et BI-dashboard og ikke en realtidsapp — det
er et statisk øjebliksbillede med samme dokumentfølelse som resten af
Rentemester.

Målgruppen er én-person- og småbedrifter (revisor eller ejerleder) som ikke
ønsker at åbne en webapp dagligt. De vil have ét overblik der kan printes,
mailes til revisor, og som ligner et bilag fra Skattestyrelsen mere end et SaaS.

## Layout-skitse

```
┌──────────────────────────────────────────────────────────┐
│ Rentemester ApS                          CVR DK12345678  │
│ Dashboard · 17. maj 2026 · Backup: 2 timer siden         │
├──────────────────────────────────────────────────────────┤
│ ╔══════════╗ ╔══════════╗ ╔══════════╗ ╔══════════╗      │
│ ║  ÅBNE    ║ ║OVERFOR-   ║ ║ULINKEDE   ║ ║ÅBNE       ║   │
│ ║  FAKTURA ║ ║FALDNE     ║ ║BANK-TX    ║ ║EXCEPTIONS ║   │
│ ║  3       ║ ║1 (45 d)   ║ ║4          ║ ║2          ║   │
│ ║ 12.500   ║ ║ 1.250     ║ ║           ║ ║           ║   │
│ ║   DKK    ║ ║   DKK     ║ ║           ║ ║           ║   │
│ ╚══════════╝ ╚══════════╝ ╚══════════╝ ╚══════════╝      │
│                                                          │
│ Næste momsperiode                                        │
│ Q2 2026 (01-04 → 30-06) · 44 dage tilbage                │
│ Est. nettomoms 6.250,00 DKK                              │
│ ─────────────────────────────────────────────────────    │
│ Åbne fakturaer                                           │
│   2026-0001  Kunde A/S      1.250,00 DKK  forfald 15-06  │
│   2026-0002  Anden ApS      8.750,00 DKK  forfald 02-07  │
│   2026-0003  Tredje I/S     2.500,00 DKK  forfald 14-07  │
│ ─────────────────────────────────────────────────────    │
│ Seneste aktivitet                                        │
│   17-05 02:09  system   backup        Backup created     │
│   17-05 01:55  cli      invoice.post  Posted 2026-0002   │
│   16-05 14:21  cli      invoice.issue Issued 2026-0002   │
│   ...                                                    │
│ ─────────────────────────────────────────────────────    │
│ Backup-status     [grøn pill] OK · 0 dage siden          │
│ Audit-chain       [grøn pill] OK · 142 entries           │
│ ─────────────────────────────────────────────────────    │
│ Commit abc1234 · rules v2026-05 · genereret 17-05 02:10  │
│ github.com/mikkelkrogsholm/rentemester                   │
└──────────────────────────────────────────────────────────┘
```

## Datakontrakt

Render-engine (#84) modtager en `DashboardInput`-struct og returnerer en
streng. Input er rent — det indeholder ingen `Date.now()`, ingen `git`-kald,
ingen filsystem-læsninger. Alle disse ting samles af CLI'en (#85) og leveres
ind.

```ts
type DashboardInput = {
  asOfDate: string;            // YYYY-MM-DD, parameter ind
  generatedAt: string;         // ISO 8601 UTC, parameter ind
  commitSha: string;           // 7-tegns short SHA, parameter ind
  ruleBundleVersion: string;   // fra currentRuleBundleVersion()
  company: CompanySettings;    // fra getCompanySettings(db)
  invoices: InvoiceListResult; // fra buildInvoiceList(db, { status: 'open', asOfDate })
  overdueInvoices: InvoiceListResult; // fra buildOverdueInvoiceList(db, { asOfDate })
  unlinkedBank: BankTransactionListResult; // fra listBankTransactions(db, { status: 'unmatched' })
  exceptions: ListExceptionsResult; // fra listExceptions(db, { status: 'open' })
  vatPeriod: VatPeriodReport;  // fra buildVatReport(db, currentPeriodStart, currentPeriodEnd)
  vatDaysRemaining: number;    // udregnet fra asOfDate vs periodEnd
  recentActivity: AuditLogRow[]; // 10 seneste fra audit_log (read-only)
  backup: BackupComplianceStatus; // fra getBackupComplianceStatus(db, root, asOfDate)
  audit: { ok: boolean; entryCount: number; firstError?: string }; // fra verifyAuditChain(db)
};
```

## Sektioner

For hver sektion: hvilke kerne-API'er den kalder, hvilke felter den viser,
edge-cases, og hvilke `DESIGN.md`-tokens den bruger.

### 1. Header

**Kerne-API:** `getCompanySettings(db)` (`src/core/company.ts`),
`getBackupComplianceStatus(db, companyRoot, asOf)` (`src/core/system-backups.ts`).

**Felter:**

- Virksomhedsnavn (`company.name`)
- CVR formateret som `CVR DK12345678` (`company.cvr`)
- Dato (parameter `asOfDate`, formateret som "17. maj 2026")
- "Backup: 2 timer siden" eller "Backup: 3 dage siden"
  (`backup.latestBackupAt` vs. `asOfDate`)

**Edge-cases:**

- CVR mangler → vis kun navn uden CVR-felt, ikke "CVR null".
- Ingen backup nogensinde → "Backup: ingen registreret" i `warning`-pill.
- Firmanavn = default "Rentemester company" → vises som det er; sjusk er
  brugerens ansvar.

**Tokens:** `paper-raised` (header-baggrund), `ink` (tekst), `mono-family`
(CVR), `headline-family` (firmanavn).

### 2. Status-tæller-stribe (4 metrics)

Fire kort i en række. Store mono-tal øverst, label nederst.

**Kerne-API:**

- Åbne fakturaer: `buildInvoiceList(db, { status: 'open', asOfDate })`
  (`src/core/invoice-list.ts`). Antal = `result.count`. Sum = sum af
  `row.openBalance` for alle rows.
- Overforfaldne: `buildOverdueInvoiceList(db, { asOfDate })`. Antal =
  `result.count`. Ældste = `max(row.overdueDays)`.
- Ulinkede bank-transaktioner: `listBankTransactions(db, { status: 'unmatched' })`
  (`src/core/reconciliation.ts`). Antal = `result.count`.
- Åbne exceptions: `listExceptions(db, { status: 'open' })`
  (`src/core/exceptions.ts`). Antal = rækker.

**Felter pr. kort:**

- Stort tal (antal) — `amount-lg` mono.
- Sekundærtekst (sum DKK / ældste dage) — `amount` mono.
- Label nederst — `label-sm` versaler.

**Edge-cases:**

- 0-værdier: vis `0` i `ink-muted`, ikke "ingen".
- Sum kan være 0 selv om count > 0 (fx fuldt kreditnotaet): vis `0,00 DKK`.

**Tokens:** `paper-raised` (kort-baggrund), `ink` (tal), `ink-muted` (labels),
`amount-cell` (mono-justering), `accent` ved overforfaldne > 0, `danger`-pill
ved exceptions > 0.

### 3. Næste deadline (momsperiode)

**Kerne-API:** `buildVatReport(db, periodStart, periodEnd)`
(`src/core/vat.ts`). CLI udregner aktuel kvartalsperiode fra `asOfDate` (Q1 =
jan-mar osv. — nuværende dansk standard, ikke firmaets fiscal year).

**Felter:**

- Periodelabel: "Q2 2026 (01-04 → 30-06)"
- Dage tilbage = `daysBetween(asOfDate, periodEnd)`
- Estimeret nettomoms = `vatPeriod.netVatPayable`

**Edge-cases:**

- `asOfDate` efter `periodEnd`: vis "Forfalden" i `danger`-pill.
- `vatPeriod.errors.length > 0`: vis "Kan ikke beregne" + første fejl i
  `warning`-pill.
- Negativ nettomoms (refusion): formater med fortegn, label "Til gode".

**Tokens:** `paper` (baggrund), `headline-family` (periodelabel), `amount-lg`
(beløb), `success`-pill / `warning`-pill / `danger`-pill afhængigt af dage.

### 4. Åbne fakturaer-tabel

**Kerne-API:** Samme `buildInvoiceList`-kald som metric #1, sorteret på
`effectiveDueDate ASC`.

**Kolonner:**

| Felt | Justering | Kilde |
| --- | --- | --- |
| Fakturanr. | venstre, mono | `row.invoiceNumber` |
| Kunde | venstre | `row.customerName` |
| Beløb | højre, mono | `row.openBalance` formateret `1.250,00 DKK` |
| Forfald | højre, mono | `row.effectiveDueDate` formateret `15-06` |
| Status | center, pill | `row.isOverdue ? 'overdue' : 'open'` |

**Max rækker:** 10. Hvis `result.count > 10` vises `… og 7 yderligere` som
mute-tekst nederst. Ingen JS-paginering.

**Eksempel-data** (fra smoke):

```
2026-0001  Kunde A/S       1.250,00 DKK   15-06    open
2026-0002  Anden ApS       8.750,00 DKK   02-07    open
2026-0003  Tredje I/S      2.500,00 DKK   14-07    open
2025-0042  Gammel Kunde    4.500,00 DKK   01-04    overdue (46 d)
```

**Edge-cases:**

- Ingen åbne fakturaer: vis tom-state "Ingen åbne fakturaer" i `ink-muted`,
  ikke en tom tabel.
- `effectiveDueDate` mangler: sortér først, vis `—` i kolonnen.
- `customerName` mangler: vis CVR i stedet, derefter `—`.

**Tokens:** `paper` (række-baggrund), `paper-raised` (zebra hver anden),
`table-row` (komponent-token), `amount-cell` (beløb), `badge-status-paid` /
`badge-status-overdue` (status-pill), `ink-muted` (tom-state).

### 5. Seneste aktivitet (audit_log seneste 10)

**Kerne-API:** Direkte SQL-query mod `audit_log`-tabellen (append-only, jf.
`src/core/schema.sql`). Ingen public helper findes endnu — render-engine kan
enten kalde rå SQL eller vi tilføjer en `listRecentAuditLog(db, limit)` i
samme commit som #84. Kontrakt: returnerer 10 seneste sorteret `created_at
DESC`.

**Felter pr. række:**

- Tid: `created_at` formateret `17-05 02:09`
- Aktør: `actor` (fx `system`, `cli`, `claude`)
- Event: `event_type` (fx `invoice.post`, `backup.create`)
- Tekst: `message` trunkeret til 60 tegn

**Edge-cases:**

- Færre end 10 events: vis hvad der findes, ingen padding.
- Helt tom (lige initialiseret): vis "Ingen aktivitet endnu" i `ink-muted`.
- Meget lange `message`: trunkér med `…`, hover-tooltip ikke nødvendigt i v1.

**Tokens:** `paper` (baggrund), `mono-family` (tid + event_type),
`body-family` (message), `ink-muted` (aktør).

### 6. Backup-status

**Kerne-API:** `getBackupComplianceStatus(db, companyRoot, asOfDate)`
(`src/core/system-backups.ts`).

**Felter:**

- Pill med tekst "OK" / "Snart due" / "Forfalden"
- Sekundærtekst: "0 dage siden" / "3 dage siden" / "8 dage siden"
- Sub-linje: `latestBackupAt` formateret kort

**Farve-logik:**

- `daysSinceLatestBackup === null` eller `> 7`: `danger`-pill, tekst "Forfalden".
- `daysSinceLatestBackup` mellem 5 og 7 inkl.: `warning`-pill, tekst "Snart due".
- Ellers: `success`-pill, tekst "OK".

**Edge-cases:**

- `backupsFound === 0`: `danger`-pill "Ingen backup".
- `hasActivitySinceBackup === true` og dage > 0: tilføj note "(ændringer
  siden seneste backup)".

**Tokens:** `success` / `warning` / `danger` (baggrund pill), `success-soft`
/ `warning-soft` / `danger-soft` (soft variant til pill), `ink` (tekst).

### 7. Audit-chain-status

**Kerne-API:** `verifyAuditChain(db)` (`src/core/ledger.ts`).

**Felter:**

- Pill "OK" eller "FEJL"
- Sub-linje: antal entries verificeret eller første fejlbesked

**Edge-cases:**

- Ingen entries endnu: `success`-pill med tekst "OK · 0 entries". Ikke en fejl.
- Fejl i kæden: `danger`-pill + kort uddrag (max 80 tegn) af første fejl. Hele
  fejlen kan ses via `rentemester audit verify`.

**Tokens:** `success` / `danger` (pill-baggrund), `mono-family` (entry-tal),
`alert-danger` (komponent-token) hvis fejl, `body-family` (fejlbesked).

### 8a. Åbne kreditorposter (creditor card)

**Kerne-API:** `buildPayablesList(db, { status: 'open', asOfDate })`
(`src/core/payables.ts`).

Den kreditor-side pendant til "Åbne fakturaer" (debitor). Viser de åbne
leverandørposter med åben saldo, forfaldsdato og en status-pill, plus en
sammenfatningslinje med samlet åben kreditorgæld og — hvis relevant — den
overforfaldne andel.

**Kolonner:** Bilagsnr. (mono), Leverandør, Åben saldo (højre, mono), Forfald
(højre, mono `DD-MM`), Status (`åben` / `forfalden (N d)`).

**Max rækker:** 10, `buildPayablesList` sorterer mest overforfaldne først;
overskydende vises som "… og N yderligere".

**Edge-cases:** ingen kreditorposter → tom-state "Ingen åbne kreditorposter".

**Tokens:** samme som "Åbne fakturaer" — `dash-table`, `amount-cell`,
`badge-status-*`.

### 8b. Periodeafgrænsningsposter (accruals card)

**Kerne-API:** `buildAccrualRegisterReport(db)` +
`listDueAccrualRecognitionPeriods(db, asOfDate)` (`src/core/accruals.ts`).

To status-rækker: (1) resterende balanceeksponering = summen af endnu ikke
indtægts-/omkostningsførte beløb, med antal aktive periodeafgrænsningsposter;
(2) recognition-perioder der er forfaldne (`recognitionDate <= asOfDate`) og
ikke bogført — et antal og det samlede beløb der skal periodiseres.

**Edge-cases:** ingen periodeafgrænsningsposter → tom-state.

**Tokens:** `status-row`, `amount-lg`, `success`/`danger`-pill.

### 8c. Budget & likviditet (budget & liquidity card)

**Kerne-API:** `buildBudgetVsActual(db, currentMonth, currentMonth)`
(`src/core/budget.ts`) + `buildLiquidityForecast(db, { startDate, months: 3 })`
(`src/core/liquidity-forecast.ts`). CLI'en udregner `currentMonth` som
`asOfDate`'s kalendermåned og likviditets-`startDate` som den 1. i den
efterfølgende måned.

To status-rækker: budget-vs-faktisk for den aktuelle måned (budget, faktisk,
afvigelse), og likviditetsprognosen for de kommende måneder (projiceret
slutsaldo + laveste saldo hvis den dykker negativt). En negativ projiceret
likviditet markeres med en `danger`-pill.

**Edge-cases:** intet budget / ingen prognosedata → en neutral "—"-pill med
forklarende tekst.

### 8d. Skat (tax card)

**Kerne-API:** `buildTaxReturn(db, fyStart, fyEnd)` (`src/core/tax-return.ts`)
for det regnskabsår `asOfDate` falder i — kun når året er lukket/indberettet
(`accounting_periods.kind = 'fiscal_year'`).

To tilstande: er regnskabsåret lukket vises estimeret selskabsskat, årets
resultat og antallet af needs-review-punkter; ellers vises "Forberedelse er
klar, når regnskabsåret er lukket" med en neutral pill.

### 8e. EU-salg & OSS (light indicator)

**Kerne-API:** `buildViesRecapitulativeStatement(db, periodStart, periodEnd)`
(`src/core/vat-vies-list.ts`) + `buildOssReport(db, periodStart, periodEnd)`
(`src/core/vat-oss.ts`) for den viste momsperiode.

En **let** indikator: sektionen rendres KUN når der er grænseoverskridende
B2B-salg uden moms eller OSS-klassificeret forbrugersalg i perioden — altså
noget der kræver en separat indberetning. Er begge tal 0, udelades sektionen
helt.

### 8. Footer

**Kerne-API:** `currentRuleBundleVersion()` (`src/core/rules-metadata.ts`).
Commit-SHA og generated-at leveres som parametre ind i `DashboardInput` (CLI
kalder `git rev-parse --short HEAD` selv — render-engine forbliver ren).

**Felter:**

- Commit-SHA (7 tegn)
- Rule-version
- Genereringstidspunkt formateret `17-05 02:10`
- Link-tekst til kildekode (statisk URL, ingen klikbart hvis filen åbnes
  offline — det er kun tekst)

**Edge-cases:**

- Ikke et git-repo (release-tarball): CLI sender `commitSha: 'unknown'`,
  render viser "Commit unknown".

**Tokens:** `paper-raised` (footer-baggrund), `ink-muted` (alt tekst),
`mono-family` (commit + version), `label-sm`.

## Refresh-cadence

**Beslutning: on-demand via CLI.** Ingen cron, ingen scheduler, ingen daemon.

```bash
rentemester dashboard --company <root> --out dashboard.html --as-of 2026-05-17
```

Begrundelse:

- Konsistent med resten af Rentemester: alt køres når brugeren beder om det.
- Determinisme: samme `--as-of` + samme database = samme HTML, byte-for-byte.
- Ingen tilstand at vedligeholde mellem kørsler.
- Cron-løsning kan tilføjes som tynd shell-wrapper senere uden at ændre
  spec'en (`crontab` kalder samme CLI én gang dagligt og uploader output).

`--as-of` defaulter til "i dag" hvis udeladt, men dashboardet selv er stadig
deterministisk givet samme dato.

## Sprog

**Beslutning: dansk default.** Engelsk er ikke et v1-mål. Felt-formatering:

- Datoer i tabeller: `DD-MM` (kompakt, `15-06`) eller `DD-MM-YYYY` hvis
  årstal kan være tvetydigt.
- Datoer i prosa (header, footer): `17. maj 2026`.
- Tid i seneste aktivitet: `DD-MM HH:mm` (lokal tid svarer til DK, da
  produktet ikke understøtter andre tidszoner i v1).
- Beløb: `1.234,56 DKK` med punktum som tusindsep., komma som decimalsep.,
  altid 2 decimaler, valutakode efter med non-breaking space.
- Negative beløb: minus-prefix, `-1.234,56 DKK`. Ingen parenteser.

Formaterings-utilities placeres i `src/render/format.ts` så de kan testes
isoleret og deles mellem render-engine og CLI.

## Determinisme

**Må variere mellem kørsler:** intet.

Givet samme `DashboardInput` skal render-engine producere byte-for-byte
identisk HTML. Det betyder:

- Ingen `Date.now()` inde i render-engine.
- Ingen `Math.random()`.
- Ingen iteration over `Map`/`Set` uden eksplicit sort.
- Ingen environment-variabler.
- Ingen filsystem-læsninger.

**Skal være parametre:**

- `asOfDate` — bestemmer alle "X dage siden"-udregninger.
- `generatedAt` — vises i footer, ikke brugt til logik.
- `commitSha` — vises i footer.

CLI'en (#85) er ansvarlig for at samle alle disse felter og kalde
render-engine. Render-engine eksporterer én funktion: `renderDashboard(input:
DashboardInput): string`.

**Test-strategi for #84:** snapshot-test med fast fixture-input giver
byte-identisk output ved to kørsler.

## Tokens fra DESIGN.md

Dashboardet bruger udelukkende tokens defineret i `DESIGN.md`. Ingen nye
farver, ingen nye spacings introduceres af dashboardet selv.

| Token | Anvendelse |
| --- | --- |
| `colors.paper` | Hovedbaggrund |
| `colors.paper-raised` | Header, footer, metric-kort, zebra-rækker |
| `colors.ink` | Primær tekst, store tal |
| `colors.ink-muted` | Labels, sekundærtekst, tom-states |
| `colors.accent` | Sparsom fremhævning af overforfaldne |
| `colors.success` / `success-soft` | OK-pills (backup, audit) |
| `colors.warning` / `warning-soft` | Snart due-pills |
| `colors.danger` / `danger-soft` | Fejl-pills (forfalden backup, audit-fejl, exceptions > 0) |
| `typography.headline-family` | Firmanavn, sektions-overskrifter |
| `typography.body-family` | Brødtekst, kunde-navne |
| `typography.mono-family` | Beløb, datoer, CVR, commit-SHA, event_type |
| `typography.mono-features: tnum` | Sikrer tabular figures i tabeller |
| `spacing.{xxs..xl}` | 8px-grid; ingen ad hoc-værdier |
| `rounded.sm` | Pills |
| `rounded.md` | Metric-kort, tabeller |
| `components.table-row` | Tabel-rækker |
| `components.amount-cell` | Beløb-celler |
| `components.badge-status-paid` | Status-pill open/paid |
| `components.badge-status-overdue` | Status-pill overdue |
| `components.alert-danger` | Audit-chain-fejl |

Nye semantiske størrelser dashboardet introducerer (skal tilføjes til
`DESIGN.md` i #84 hvis ikke allerede der):

- `amount-lg` — stor mono-størrelse til hero-tal i metric-kortene.
- `label-sm` — versaler-label under hero-tal.

Disse er rent typografiske og bør kunne afledes fra eksisterende
`body-size` + en multiplikator, ikke nye farver.

## Hvad #84 (render-engine) skal bygge

1. `src/render/dashboard.ts` med `renderDashboard(input: DashboardInput): string`.
2. `src/render/format.ts` med dato- og beløbsformaterings-utilities.
3. Statisk CSS inlinet i HTML — ingen eksterne stylesheets.
4. Snapshot-test mod en fast `DashboardInput`-fixture.
5. Visuel diff-test: render med tom database giver gyldig HTML (alle
   tom-states virker).

## Hvad #85 (CLI) skal bygge

1. `rentemester dashboard` subcommand.
2. Argumenter: `--company`, `--out`, `--as-of` (default = i dag).
3. Samle `DashboardInput` ved at kalde alle kerne-API'erne ovenfor.
4. Køre `git rev-parse --short HEAD` (best-effort, ellers `'unknown'`).
5. Skrive HTML til `--out`-stien atomisk via `atomic-file.ts`.
