# Confirm-konventionen — én tabel på tværs af MCP, cockpit og CLI (#369)

## Rust remote / native MCP

Tabellerne nedenfor beskriver den tidligere TypeScript-overflade. I den native
leverandørarbejdsgang kræver `remote register-payable` og `remote pay-payable`
bar `--confirm` samt native JSON-body med `confirm:true`. MCP kræver outer
`confirm:true` samt den samme bekræftede native body. Serveren håndhæver den
aktuelle bruger-/serviceautoritet. De nye agentopslag udfører ingen mutation
og accepterer ikke et bekræftelsesflag. Brug det faktiske native katalog og
[serverkontrakten](rust-server.md); udled ikke native retry-regler af de gamle
kommandoers navne.

Rentemester har tre kalde-overflader (MCP, cockpit HTTP API, CLI) der hver
har deres egen `confirm`-konvention. Det er **bevidst** — overfladerne har
forskellige UI-mæssige forudsætninger (cockpit har en modal som samtykke,
CLI er en shell, MCP er en stateless tools/call) — men en agent skal kunne
slå op på ét sted hvilken regel der gælder for samme business-operation
på hver stak.

Dette dokument er det opslag. De tre stak-kontrakter
([`mcp-agent-contract.md`](mcp-agent-contract.md),
[`cockpit-api.md`](cockpit-api.md),
[`cli-contract.md`](cli-contract.md)) linker hertil og holder sig til de
regler der står her.

## Princippet

| Stak | Confirm-signalet | Hvornår kræves det |
|------|------------------|---------------------|
| **MCP** | `confirm: true` i tool-argumentet (boolean) | **Alle** write- og destructive-tools. Uden det returneres `{ ok:false, errors:["confirm: true required for write tool <name>"] }` (eller `… destructive tool …` for `system_restore_backup`). |
| **Cockpit** | `"confirm": true` i request-body (JSON boolean) | Kun **irreversible** writes. `POST /invoices/issue` og `POST /exceptions/:id/resolve` kræver det **ikke** — modalen er menneskets samtykke. Uden det: `400 bad_request` med `denne handling er irreversibel og kræver 'confirm: true'`. |
| **CLI** | `--confirm yes` valued flag (literal `"yes"`) | Kun for **destructive** kommandoer der står i tabellen nedenfor. De øvrige writes har actor-politikken (`--actor`) som samtykke. Uden flaget: exit `1`, `{ok:false, errors:["… Re-run with --confirm yes to proceed."]}`. |

**Hvorfor det er forskelligt.** Cockpit har en modal foran enhver irreversibel
handling — modalen er det faktiske samtykke, så et separat `confirm`-felt på
de skema-validerede mutationer (`bank/import`, `documents/ingest`,
`invoices/post`, `invoices/settle`, …) er en ekstra spærre mod fejl-POST,
ikke selve samtykket. CLI er en shell hvor `--actor` allerede dokumenterer
beslutningen; et ekstra `--confirm yes` på almindelige bogføringer ville være
støj. MCP har ingen modal og ingen interaktiv shell, så **alle** writes
gates af `confirm: true` — det er det eneste signal en agent kan give.

## Tabellen — per business-operation

Hver række er én logisk mutation. **Kræves** betyder afvisning uden samtykke.
**N/A** betyder operationen findes ikke på den stak.

| Business-operation | MCP-tool / cockpit-route / CLI-kommando | MCP | Cockpit | CLI |
|--------------------|-----------------------------------------|-----|---------|-----|
| Udsted faktura (kladde) | `invoice_issue` / `POST /invoices/issue` / `invoice create` | `confirm: true` | **Ikke krævet** (modal er samtykket) | Ikke krævet (`--actor` er samtykket) |
| Bogfør faktura | `invoice_post` / `POST /invoices/post` / `invoice post` | `confirm: true` | `confirm: true` | Ikke krævet |
| Reparér legacy-fakturabogføring | *(ingen MCP-tool)* / *(N/A)* / `invoice repair-posting` | N/A | N/A | Ikke krævet (`--actor`, eksplicit journal-id og `--reason` kræves) |
| Bogfør betaling fra bank | `invoice_settle_bank` / `POST /invoices/settle` / `invoice settle-bank` | `confirm: true` | `confirm: true` | Ikke krævet |
| Krediter faktura | `invoice_credit_note` / `POST /invoices/credit-note` / `invoice credit-note` | `confirm: true` | `confirm: true` | Ikke krævet |
| Afskriv tab på debitor | `invoice_write_off_bad_debt` / *(N/A)* / `invoice write-off-bad-debt` | `confirm: true` | N/A | Ikke krævet |
| Send faktura på e-mail | `invoice_send_email` / `POST /invoices/send` / `invoice send` | `confirm: true` | `confirm: true` | Ikke krævet |
| Send rykker | `invoice_remind` / `POST /invoices/send-reminder` / `invoice remind` | `confirm: true` | `confirm: true` | Ikke krævet |
| Importer bank-CSV | `bank_import` / `POST /bank/import` / `bank import` | `confirm: true` | `confirm: true` | Ikke krævet |
| Ret bankafstemning | `bank_reconciliation_correction_apply` / `POST /bank/reconciliation-correction` / `bank correction-apply` | `confirm: true` | `confirm: true` | `--confirm yes` |
| Ingester bilag | `documents_ingest` / `POST /documents/ingest` / `documents ingest` | `confirm: true` | `confirm: true` | Ikke krævet |
| Bogfør finanspostering | `journal_post` / *(N/A)* / `journal post` | `confirm: true` | N/A | Ikke krævet |
| Modpost finanspostering | `journal_reverse` / *(N/A)* / `journal reverse` | `confirm: true` | N/A | Ikke krævet |
| Bogfør udgift | `expense_book` / *(N/A)* / `expense book` | `confirm: true` | N/A | Ikke krævet |
| Luk periode | `period_close` / `POST /periods/close` / `period close` | `confirm: true` | `confirm: true` | Ikke krævet |
| Genåbn periode | *(ingen MCP-tool)* / `POST /periods/reopen` / `period reopen` | N/A | `confirm: true` | Ikke krævet |
| Ryd undtagelse | `exception_resolve` / `POST /exceptions/:id/resolve` / `exceptions resolve` | `confirm: true` | **Ikke krævet** (kun status flippes) | Ikke krævet |
| Generér tilbagevendende faktura | `recurring_invoice_generate` / `POST /recurring-invoices/generate` / `recurring-invoice generate` | `confirm: true` | `confirm: true` | Ikke krævet |
| Kør gentagende fakturaer i workspace | `recurring_invoice_run_workspace` / *(ingen cockpit-timer)* / `recurring-invoice run-workspace` | `confirm: true` | N/A | **`--confirm yes`** |
| Registrer aktiv | `asset_register` / *(N/A)* / `asset register` | `confirm: true` | N/A | Ikke krævet |
| Straksafskriv aktiv | `asset_write_off` / *(N/A)* / `asset write-off` | `confirm: true` | N/A | **`--confirm yes`** (matcher det MCP-felt der hedder `confirmImmediateWriteOff`) |
| Tag backup | `system_backup` / *(N/A)* / `system backup` | `confirm: true` | N/A | Ikke krævet |
| Genskab fra backup | `system_restore_backup` (**destructive**) / *(N/A)* / `system restore-backup` | `confirm: true` **+ `confirmText: "RESTORE <targetCompany>"`** | N/A | **`--confirm yes`** |
| GDPR-indsigtsrapport | `gdpr_export` / `POST /gdpr/export` / `gdpr export` | `confirm: true` | `confirm: true` | Ikke et confirm-flag, men actor er påkrævet; eksporten append-only audit-logges |
| GDPR-slet persondata | *(ingen MCP-slette-tool — bevidst CLI/Cockpit-only)* / `POST /gdpr/erase` / `gdpr forget` (legacy alias: `gdpr erase`) | N/A | `confirm: true` | Ikke `--confirm yes`, men **`--after-retention-expiry`** (eksplicit flag; exit `2` uden) + actor — retention vurderes altid mod dags dato |
| Slet kontakt fra cockpittet | *(N/A)* / `DELETE /contacts/:id` / *(N/A)* | N/A | `confirm: true` | N/A |

(Tabellen er ikke udtømmende for alle 114 MCP-tools — 49 read, 64 ordinary
writes og 1 destructive; alle 65 ikke-read tools er confirm-gatede. Den dækker de
business-operationer der har en konflikt eller en afvigelse mellem stakke.
For den fulde liste pr. tool, se `annotations` i `docs/mcp-tool-surface.md`.)

## Afvisningsbeskeder — sprog og string-match

Beskederne er bevidst forskellige (engelsk på MCP, dansk på cockpit), fordi
de er rettet mod forskellige målgrupper (MCP: ekstern agent; cockpit:
slut-bruger). En agent der string-matcher skal håndtere begge:

| Stak | Substreng der altid optræder ved manglende confirm |
|------|----------------------------------------------------|
| MCP — write | `confirm: true required for write tool ` *(prefix; tool-navnet følger)* |
| MCP — destructive | `confirm: true required for destructive tool ` *(prefix; ordet `destructive`, ikke `write`)* |
| MCP — fælles prefix (matcher begge ovenstående) | `confirm: true required for ` |
| Cockpit | `denne handling er irreversibel og kræver 'confirm: true'` *(eksakt, dansk)* |
| CLI — `asset write-off` | `Re-run with --confirm yes to proceed.` *(suffix; output i `errors[]`)* |
| CLI — `system restore-backup` | `Re-run with --confirm yes to proceed.` *(suffix; output i `errors[]`)* |

**Anbefaling — branch på `code`, ikke på strengen.** På MCP bærer
afvisnings-konvolutten en stabil, maskinlæsbar `code`-markør, og det er
den en agent skal branche på:

| Envelope-`code` | Betydning |
|---|---|
| `CONFIRM_REQUIRED` | `confirm: true` manglede — sættes for **både** write- og destructive-tools (dækker begge tekst-varianter ovenfor). |
| `CONFIRMTEXT_MISMATCH` | `confirmText` manglede/var forkert på `system_restore_backup`. |
| `BACKUP_LOCKED` | Bogførings-låsen blokerede en write (ikke en confirm-fejl, men samme konvolut-mekanisme). |

Streng-matching på `errors[]` (fx prefixet `confirm: true required for `,
der fanger både `write tool` og `destructive tool`) er kun en fallback for
klienter der ikke læser `code`. Cockpit og CLI har ingen `code`-markør:
match cockpit på den eksakte danske streng og CLI på suffixet
`Re-run with --confirm yes to proceed.`. Match aldrig på sprog — beskederne
skifter sprog mellem stakke.

## `invoice_issue` vs. `POST /invoices/issue` — afvigelsen er bevidst

Samme business-operation, modsat regel — og det er **med vilje**:

- **MCP's `invoice_issue` kræver `confirm: true`.** Der er ingen modal, ingen
  shell-prompt; en eksplicit `confirm` er det eneste signal en agent kan
  give om at den ikke kalder ved et uheld. Selv om `invoice_issue` kun
  producerer en kladde (intet journal-entry endnu), kræver kontrakten
  alligevel `confirm` — det er ensartet på tværs af alle 65 confirm-gatede
  tools (64 writes + det destruktive restore), så agenten ikke skal huske
  undtagelser.
- **Cockpittets `POST /invoices/issue` kræver det IKKE.** Den multi-linje
  faktura-modal i SPA'en *er* samtykket — at trykke "Udsted faktura"-knappen
  efter at have udfyldt linjerne er den menneskelige beslutning.
  `POST /exceptions/:id/resolve` følger samme logik (kun status flippes;
  modalen er samtykket).
- **CLI's `invoice create`/`invoice issue` kræver det IKKE.** `--actor`
  er allerede den eksplicitte beslutning; et ekstra `--confirm yes` ville
  være støj på en daglig handling.

En agent der internaliserer "alle MCP writes kræver `confirm`" og
ekstrapolerer den regel direkte til cockpit tager fejl for disse to ruter.
Slå op her, eller læs hver rutes egen kontrakt-side.

## Destruktive tools — én ekstra ring

`system_restore_backup` er det eneste **destructive** tool. Det kræver:

- `confirm: true` (som alle writes), og
- `confirmText` der er den **eksakte streng** `RESTORE <targetCompany>`
  (fx `RESTORE acme-aps` hvis target-company-stien hedder `acme-aps`).

Manglende `confirm` returneres som
`confirm: true required for destructive tool system_restore_backup` — bemærk
ordet `destructive`, **ikke** `write`. Konvolutten bærer dog samme stabile
`code: "CONFIRM_REQUIRED"` som write-tools, så en agent der brancher på
`code` rammer begge uden videre. En streng-matchende agent der kun matcher
`required for write tool` springer denne fejl over; match i så fald på
prefixet `confirm: true required for ` for at fange begge.
`confirmText`-fejlen bærer `code: "CONFIRMTEXT_MISMATCH"`.

CLI's pendant er `system restore-backup --confirm yes` (suppleret med
`--target-company <path>` og `--actor`). Det er CLI-konventionen — der er
ingen `confirmText`-ækvivalent fordi `--target-company` allerede er en
eksplicit sti agenten selv skrev.

## Hvorfor `confirm: true` (boolean) på MCP og cockpit, men `--confirm yes` (string) på CLI

CLI'ens `cli-args.ts` har et **append-only `BOOLEAN_FLAGS`-sæt** der ikke
må udvides (alt nyt skal være valued for at undgå utilsigtede tolkninger
af `--confirm` i den eksisterende parser). Den eneste back-kompatible
form for confirm på CLI er derfor en *valued* flag (`--confirm yes`).
Det er **dokumenteret som ækvivalent** med MCP/cockpit's `confirm: true`
— samme intention, anden syntax.

## Hvor implementeringen ligger

- **MCP:** `confirmField` + `withCompanyDbConfirmed` + `withDestructiveConfirm`
  i `src/mcp/tool-runtime.ts`.
- **Cockpit:** `requireConfirm: true` option på `withCompanyMutation` i
  `src/server/mutations.ts` (confirm-gate-trinnet i write-pipelinen).
- **CLI:** Pr. kommando i `src/cli/system.ts` (`restore-backup`),
  `src/cli/asset.ts` (`write-off`). `ctx.arg("--confirm")` læses som streng
  og sammenlignes mod `"yes"`.

Hvis du tilføjer en ny muterende operation: lås den til den konvention der
gælder for stakken (se "Princippet" ovenfor), og opdater tabellen i dette
dokument.
