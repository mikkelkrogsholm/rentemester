# CLI-kontrakt: actor-politik og exit-koder

## Rust-klienten og agentopslag

### Lokal original `system healthcheck` (Rust)

`rentemester-rust system healthcheck --company PATH [--json | --format json|human]`
er en endelig offline read-only diagnose. `--company` er obligatorisk; hverken
miljøvariabler, aktuel profil eller en standardvirksomhed erstatter stien.
Manglende virksomheder diagnosticeres uden at blive oprettet. Kommandoen tager
ikke server-, cookie-, local-profile-, actor-, stdin-, repair- eller mutationsinput;
ukendte flag, dubletter og ugyldige værdier afvises med exit 2 før datalæsning.
Globale flag kan stå før eller mellem kommandoordene; company/format understøtter
både `--flag værdi` og `--flag=værdi`. `--json` vinder over `--format human`.
Remote health er kun aggregatstatus og kan ikke erstatte denne virksomhedsrapport.

Rapporten er flad: `{ok,errors,checks:[{name,ok}],missing,schema_outdated,schema?}`.
De fem checks står altid i rækkefølgen `company_root` (root), `data_dir`
(root/data), `ledger` (root/data/ledger.sqlite), `documents`
(root/documents/inbox), `config` (root/config). Forkerte filtyper og symlinks
på disse komponenter (inklusive documents-mellemleddet) accepteres ikke.
Uden ledger udelades `schema`, og `schema_outdated` er false. Schema indeholder
`{status,currentVersion,requiredVersion,pending:[{id,name,checksum}],error?}`;
requiredVersion kommer fra det indlejrede katalog. Kun et eksakt checksummet
kanonisk historisk prefix med gyldigt fysisk schema og integritet giver pending.
Current kræver eksisterende native current-validering, inklusive kendte extensions.
Højere observeret numerisk version giver newer uden pending eller skriveautoritet.
Altereret/ukendt fysisk schema eller historik er corrupt; utilgængelig/beskadiget
snapshot eller ikke-kvalificeret ikke-Unix-platform er unavailable. Fejltekster er
stabile, sanitiserede native beskrivelser, ikke rå SQLite-fejl eller lokale stier.

SQLite åbner alene en beskyttet privat byte-snapshot; originalen og dens sidecars
ændres ikke. Ingen migration, reparation, serverstart eller skriveautoritet.
Kun eksplicit `--json` eller `--format json` vælger pretty-printed JSON på stdout
uden wrapper (også ved exit 1). En pipe alene ændrer ikke denne kommandos output.
Human-output er præcis `OK|FAIL name` pr. check og eventuelt
`OK|FAIL schema_STATUS current=N required=N`, på stdout også ved fejl, uden footer.
Exit 0 betyder alle checks og current schema; exit 1 er et diagnostisk fund;
exit 2 er en brugsfejl på stderr.

Resten af dette dokument beskriver TypeScript-CLI'en. For `rentemester-rust
remote` gælder den [native serverkontrakt](rust-server.md). `remote --help`
viser de faktiske kommandoer og inputkontrakter. Remote-writes tilskrives den
autentificerede bruger eller serviceidentitet på serveren; klienten må ikke
opfinde en actor. Leverandørregistrering/-betaling kræver både bar `--confirm`
og `confirm:true` i den native JSON-body fra stdin. Overfør ikke den gamle
CLI's `--confirm yes`-syntaks eller generelle retry-antagelser til denne klient.

`remote about`, `remote agent-capabilities --query payable --limit 1` og
`remote agent-workflow --id supplier-payable-handling` er offentlige read-only
opslag; angiv altid `--server`, samt `--allow-loopback-http` ved lokal HTTP.
De returnerer `{ok:true,data:…}`. Kataloget er udtrykkeligt delvist. Beskrivelsen
autoriserer ingen mutation; følg de aktuelle værktøjsskemaer, adgangskrav og
readback-/stopbetingelser. CLI-fejl bruger fortsat 2 for brugsfejl og 1 for
transport-/serverafvisning.

Dette dokument beskriver to forudsætninger, som ikke fremgår af den enkelte
kommandos `--help`, men som en agent skal kende for at kalde `rentemester`-CLI'en
korrekt. Implementeringen ligger i `src/cli-actor.ts` og `src/cli.ts`.

## 1. Actor-politik for muterende kommandoer

Enhver **muterende** kommando (alt der skriver til ledger'en — fakturaer,
finansposteringer, backups, kunde-/leverandøroprettelse osv.) kræver en kendt
actor. Det fulde sæt ligger i `MUTATING_COMMANDS` i `src/cli-actor.ts`.
Read-only-kommandoer (lister, rapporter, `--help`, `--example`) kræver ingen
actor.

En muterende kommando uden actor afvises med:

```
actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/USERNAME/OPENCLAW_AGENT set
```

Actoren bestemmes i denne rækkefølge:

1. `--actor <id>` — eksplicit flag. Skal være på kanonisk form
   `user:<id>`, `agent:<id>` eller `system:<id>`, og skal stå i
   `config/policy.yaml` under `actor_allowlist`. En actor uden for allowlisten
   afvises med en klar fejl.
2. `RENTEMESTER_ACTOR` miljøvariabel — behandles som et eksplicit, kanonisk
   actor-id (samme allowlist-krav som `--actor`).
3. En **udledt** actor fra miljøet, hvis intet eksplicit er sat (ingen
   allowlist-kontrol):
   - `OPENCLAW_AGENT` → `agent:<værdi>`
   - `RENTEMESTER_AGENT` → `agent:<værdi>`
   - `RENTEMESTER_USER` → `user:<værdi>`
   - `USER` → `user:<værdi>`
   - `LOGNAME` → `user:<værdi>`
   - `USERNAME` → `user:<værdi>` (Windows har hverken `USER` eller `LOGNAME`)

Findes ingen af delene, fejler kommandoen før den rører virksomhedsdata.

`--actor-via <kilde>` er valgfri og sætter `RENTEMESTER_ACTOR_VIA` (sporing af,
hvilken kanal mutationen kom igennem; standard `rentemester-cli`).

For `system restore-backup` håndhæves politikken mod `--target-company`-stien,
ikke `--company`, fordi det er dér data skrives.

## 2. Exit-koder

CLI'en bruger to fejl-exit-koder, så en agent kan skelne "jeg kaldte den
forkert" fra "kaldet var korrekt, men ledger'en afviste det":

| Exit-kode | Betydning | Eksempler |
|-----------|-----------|-----------|
| `0` | Succes. Resultatet har `ok: true`. | Postering bogført, backup oprettet. |
| `2` | Parse-/brugsfejl. Kaldet kom aldrig så langt som til forretningslogikken. | Ukendt flag, manglende påkrævet flag, ugyldigt `--format`, ukendt kommando, actor afvist af politikken, manglende `--input`-fil-argument. |
| `1` | Forretnings-/ledger-afvisning. Kaldet var velformet, men resultatet er `ok: false`. | Ubalanceret postering, faktura findes ikke, periode er lukket, `system restore-backup` uden `--confirm yes`. |

Praktisk for en agent:

- **Exit `2`** → ret selve kald'et (flag, argumenter, input-sti).
- **Exit `1`** → kald'et var korrekt; læs `errors[]` i JSON-resultatet for at se,
  hvorfor ledger'en afviste det, og ret payloaden eller forudsætningerne.
- **Exit `0`** → mutationen lykkedes.

Resultatet skrives altid til stdout (JSON med `--format json`/`--json`).
Parse-/brugsfejl (`exit 2`) skrives til stderr.

## 3. Confirm-flag

CLI'ens `confirm`-konvention er **anderledes** end MCP's og cockpit's, men
ækvivalent i intention. Slå op i [`docs/confirm-contract.md`](confirm-contract.md)
for den tabel der pr. business-operation viser hvilke stakke der kræver hvad.

**Reglen for CLI:**

- CLI'ens cli-args-parser har et **append-only `BOOLEAN_FLAGS`-sæt** (i
  `src/cli-args.ts`) der ikke må udvides. `--confirm` er derfor en **valued**
  flag — ikke en bar boolean — og værdien skal være den eksakte streng
  `yes` (`--confirm yes`). Andre værdier (`true`, `1`, tom, mangler)
  behandles som "ikke bekræftet".
- `--confirm yes` er ækvivalent med MCP's `confirm: true` og cockpit's
  `"confirm": true` i request-body. Samme intention — eksplicit samtykke
  fra agenten/operatøren før en destruktiv handling — anden syntax.
- CLI bruger `--confirm yes` på **destruktive** kommandoer, ikke på
  almindelige writes — `--actor` er allerede den eksplicitte beslutning
  for daglige bogføringer.

**Kommandoer der kræver `--confirm yes`:**

| Kommando | Hvad det gater | Fejl uden flaget |
|----------|----------------|------------------|
| `system restore-backup` | Overskriver filer i `--target-company` | Exit `1`. `errors[]` slutter med `Re-run with --confirm yes to proceed.` |
| `asset write-off` | Straksafskriver et aktiv (modposterer cost) | Exit `1`. Resultatet er `{ok:false, errors:[…]}` fra core'en. |
| `efaktura konfigurer` / `onboard` / `registrer*` | Gemmer secret eller ændrer ekstern DigiSense/NemHandel-registrering | Exit `1`; ingen secret/state/netværksmutation udføres. |
| `efaktura modtag` / `modtag-workspace` | Henter og indlæser eksterne bilag i én eller flere ledgers | Exit `1`; workspace-varianten preflighter actor og backup-lås for alle aktive mål før første netværkskald. |
| `expense vat-preflight --apply yes` | Henter nødvendig EU-VAT-evidens før købspostering | Kræver actor; uden `--apply` er samme kommando en ikke-mutérende dry-run. |
| `system migrate --apply yes` | Anvender ventende, checksummede ledger-migreringer | CLI-only; kræver actor. Uden `--apply` er kommandoen en strikt read-only plan og returnerer succes med `wouldMigrate`. Apply tager én `IMMEDIATE`-transaktion, genlæser status under låsen og er no-op hvis schema allerede er aktuelt. Ugyldige tilstande afvises uden writes; en ventende migration skal nå aktuel status og får præcis én actor-attribueret `schema_migrated`-audit-hændelse før commit. |
| `system repair-schema-views --apply yes` | Reparerer drift i indbyggede SQL-views | CLI-only; kræver allowlisted actor og `--reason` på 1–1000 tegn. Uden `--apply` returneres kun en read-only plan. Apply låser med `BEGIN IMMEDIATE`, reparerer kun den indbyggede katalogliste, verificerer igen og skriver præcis én `schema_views_repaired`-audit-hændelse atomisk. |

`system healthcheck` er altid read-only. Med `--json` eller `--format json`
udskrives én stabil JSON-resultatlinje med `checks`, `missing` og `schema`.
En ventende schema returnerer exit 1 og `schema_outdated`; `schema` indeholder
`currentVersion`, `requiredVersion` og de ventende migrationsidentiteter.
| `invoice transmit-digisense` / `efaktura leveringsstatus` | Leverer én e-faktura eller observerer eksisterende queued levering | Exit `1`; en queued identitet må kun status-poll'es og må aldrig blindt leveres igen. |
| `workspace-access bootstrap-first` | Opretter første hosted workspace-identitet | Exit `1`; password-fil og database læses ikke før eksakt `--confirm yes`. |

Alle andre muterende kommandoer (faktura-bogføring, journal-postering,
bank-import, periode-luk, …) kræver **ikke** `--confirm yes` — `--actor`
er kontrakten. DigiSense-kommandoerne ovenfor er en bevidst ekstra ekstern
sikkerhedsgate. Det modsatte gælder for samme operation på MCP (alle writes
kræver `confirm: true`); afvigelsen er bevidst og forklaret i
[`docs/confirm-contract.md`](confirm-contract.md).

## 4. Output-felter ved succes

### Leverandøridentitet i bilagsmetadata

Ved `documents ingest --metadata <fil.json>` angives en udenlandsk leverandør
som `sender.countryCode` (ISO alpha-2) og `sender.identifierKind`
(`dk_cvr`, `eu_vat` eller `non_eu`). `non_eu` må bevidst mangle
`sender.vatOrCvr`; EU reverse charge kræver fortsat et EU-momsnummer med
eksisterende VIES-evidens. Uafklaret eller modstridende land/identitet afvises
med `human_resolution_required`.

Hverken exit-koder eller `--json`-outputtets felter står i den enkelte
kommandos `--help` — per-kommando-hjælpen dækker brug, inputnoter og
tilladte flags. Exit-koderne dækkes af den **globale** hjælp
(`rentemester --help`, sidste linje) og af §2 i denne kontrakt;
output-felterne dækkes af dette afsnit.

### Write-kommandoer: felt-paritet med MCP, men FLAD form

For **write-kommandoer** gælder felt-paritet med MCP: et `--json`-succes-output
indeholder de samme resultat-felter som den tilsvarende MCP-tools `data` —
`journal post` ⇄ `journal_post`, `invoice issue` ⇄ `invoice_issue` osv.

**Men formen er flad, ikke `data`-indpakket.** CLI'en lægger resultat-felterne
direkte på topniveau sammen med `ok`/`errors`/`appliedRules`, hvor MCP wrapper
dem i `data`. En parser skrevet til MCP-konvolutten får `undefined` på CLI'en:

```jsonc
// CLI: journal post --json  (verificeret output)
{ "ok": true, "appliedRules": ["DK-BOOKKEEPING-BALANCED-001", "…"], "errors": [],
  "entryId": 1, "entryNo": "2026-00001", "entryHash": "246b…70f4" }

// MCP: journal_post — samme felter, men under `data`
{ "ok": true, "data": { "entryId": 1, "entryNo": "2026-00001", "entryHash": "246b…70f4" },
  "errors": [], "appliedRules": ["…"] }
```

Den autoritative per-tool feltliste står i
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md) under "`data`-felter pr.
tool" (samt i kildens `*Result`-typer i `src/core/*.ts`). Slå
write-kommandoens MCP-pendant op dér — og læs felterne fra CLI'ens
**topniveau**, ikke fra et `data`-felt.

### Read-kommandoer: INGEN paritet — formen varierer pr. kommando

For **read-kommandoer** gælder paritetsløftet IKKE. De har hverken en fælles
konvolut eller MCP's feltnavne: output-formen varierer pr. kommando, og
feltnavnene er **snake_case** (databasekolonner), hvor MCP bruger camelCase.
Verificerede eksempler:

```jsonc
// journal list --json — et BART array uden konvolut (ingen ok/errors);
// MCP's journal_list giver { ok, data: { entries: [...], total, … } } i camelCase
[ { "id": 1, "entry_no": "2026-00001", "transaction_date": "2026-01-15",
    "text": "Testpostering", "currency": "DKK", "amount_dkk": null,
    "document_id": null, "status": "posted", "reversal_of_entry_id": null, … } ]

// accounts list --json — konvolut-agtig, men nøglen er `rows` og felterne snake_case;
// MCP's accounts_list giver { data: { accounts: [{ accountNo, … }], count } }
{ "ok": true, "count": 48,
  "rows": [ { "account_no": "1000", "name": "Omsætning, ydelser",
              "type": "income", "default_vat_code": null }, … ] }

// invoice list --json — endnu en variant: rows + ekstra topniveau-felter
{ "ok": true, "count": 0, "status": "all", "rows": [], "errors": [] }
```

En agent der parser read-output skal altså behandle hver kommandos form
konkret (kør kommandoen mod en test-virksomhed, eller læs `src/cli/<domæne>.ts`)
— ikke udlede den fra MCP-tool-pendant'en.

Bemærk desuden de få CLI-only-kommandoer uden MCP-pendant (fx `invoice create`,
`invoice export-public`); de er listet under "CLI/MCP-mapping" i
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md).

## Rust compatibility: original `audit verify` (#716)

`rentemester-rust audit verify --help` discovers the original read-only diagnostic.
Select an explicit `--server ORIGIN` or verified private `--local-profile PATH`,
an explicit `--company SLUG` (or verified local company path), and exactly one
private `--cookie-file PATH` / `--service-key-file PATH`. Split and attached
selectors and global output flags use the original argument pipeline. There is
no actor override, stdin input, confirmation, repair, retry or local fallback.
The existing server's CompanyRead permission and native five-component verifier
remain authoritative; no separate CLI verifier is introduced.

For a validated, available native v1 observation, original output is flat
`{ok,entries,errors}`. Clean findings exit 0; corrupt findings exit 1 (not a green
observation wrapper). JSON is pretty-printed on stdout with empty stderr; human
success is on stdout and human failure on stderr, using original emitResult
labels and summaries. Usage errors exit 2 on stderr. Network, authentication,
credential, protocol, local binding and unavailable observations fail closed
with catch-shaped `{ok:false,errors}` (exit 1); raw responses and credentials are
not printed, and no entry count is invented.

**Versioned compatibility boundary:** the original Bun offline pending-schema
branch emits a truthful inspection `schema` object. Native audit verification v1
does not expose that inspection metadata. This Rust adapter therefore cannot
reproduce that branch: it refuses with the catch-shaped output, omitting both
`schema` and `entries`, rather than synthesizing migration versions or reading
SQL locally. Full #716/launcher compatibility remains separate work; the Bun
production launcher is unchanged. Native `remote audit-verify` and the SDK keep
their existing successful-observation contract (exit 0 can contain failed audit
findings under `data.audit`).

The offline Rust healthcheck also refuses symlink ancestors of the explicit company path before inspecting company facts. Select the actual canonical directory (for example `/private/var/...` rather than the macOS `/var` alias). This does not canonicalize or follow an untrusted selector.

## Rust original registry CLI compatibility

`corporate-record ingest|list|inspect|download|link|enrich|supersede` and
`ownership query|propose|review|apply|history|projection` use the existing Rust
server through an explicit `--company`, private human `--cookie-file` and
`--server` or `--local-profile`. With a local profile, `--workspace` is only
an optional equality assertion against that verified runtime; a remote call
cannot open or select a local workspace. Service-key syntax is discoverably
unsupported for this profile. Actor/principal arguments assert the current
cookie user and confer no authority; the server owns independent review,
company visibility, backup gates and exact snapshot/diff hashes.

Writes require the valued `--confirm yes`. Ingest reads bounded JSON metadata
and exact original bytes; ownership proposal reads bounded JSON facts. These
commands preserve flat original output and JSON/human streams, with exits
0/1/2, and never automatically retry a write. Malformed or mismatched write
receipts remain outcome-unknown. Originaldownload verifies base64/SHA256;
projection refuses absent native full projection. Native `remote ...`
commands retain their own wrappers. Whole #716 and the default launcher
transition remain unfinished; this profile is not a cutover or a waiver of
remaining original compatibility/dependency requirements.

## Rust explicit party merge compatibility

`party propose-merge --from-party-id ID --into-party-id ID --review-assertion TEXT --confirm yes`
and `party approve-merge --from-party-id ID --proposal-hash SHA --confirm yes`
use the common Rust server with explicit company, private human cookie and server
or verified local profile. Optional actor/workspace values assert the current
credential/runtime; they grant no authority. Approval uses the exact retained
proposal hash and keeps both identities, roles and accounting records intact.
Both parties' full company scopes and backup checks apply. The same human may
propose and approve. Flat original JSON/human output and0/1/2 exits are retained;
unknown writes require explicit history reconciliation and are never retried
automatically. Native remote CLI/MCP expose the same two operations with their
existing wrappers. Whole716/717 and the default launcher transition remain open.

## Rust original workspace inbox profile

`workspace-inbox list|inspect|status|ingest|assign|complete` uses explicit company,
private human cookie and server or verified local Unix profile. Ingest takes
`--input JSONFILE --file BYTES --idempotency-key TEXT --confirm yes`; assign and
complete take an explicit source ID and target company. Actor/workspace values
only assert the current credential/runtime. Flat JSON, literal command human
labels and0/1/2 exits are retained; status is an inspect alias. Unknown or
mismatched write receipts require canonical readback and are never retried.

This native profile supports upload, bounded native filenames, strict native
pagination and full-intent/principal replay checks. It never relabels mail or
connector traffic, truncates a filename or weakens controls. Arbitrary staged
metadata does not establish document-completion eligibility; completion uses
existing server validation/handoff and creates no journal posting. New leaf
help corrects the old handlers' missing individual discovery metadata. The
remaining source cases and whole #716/launcher transition remain open. Existing
company-knowledge aliases retain their native wrappers/stdin contracts.

## 2026-10-25 — Original AgentRun through the common Rust server

The native `rentemester-rust agent run` now executes the complete existing loop:
`ingest → book → route → payables → reconcile → deadlines → report`.
It uses the same authoritative Rust engine/server as web and Tauri. It is an
explicit financial run, including guarded expense booking and corroborated
creditor payments; it is not a proposal-only mode or a new scheduled agent.

Use the existing explicit `--server` or verified `--local-profile`, current
`--cookie-file` or `--service-key-file`, `--company`, required `--as-of`, and
optional original `--inbox`, `--metadata-dir`, `--bank-csv`, `--json`/`--format`.
The invocation supplies its transport confirmation; there is no new CLI
`--confirm` flag. `--actor` only asserts the actual credential-derived principal.
The native report contains the original15fields, a canonical company slug and
actual `user:`/`service_account:` identity. Journal attribution uses
`rentemester-runtime-agent` before the existing canonical writer commits.
Default non-agent writer programs remain unchanged.

Rules are explicit trusted operator configuration, per exact company slug:
`agentPolicies[slug].supplierRules` contains `token`, `expenseAccount`,
`vatTreatment`, `label`, and required `assetLike`. Map to an existing active
company expense account; no chart, company, supplier or equipment category is
inferred as a built-in default. Empty policy routes confident matches to human
review. The HTTP request cannot supply rules, actor authority or providers.

For `--bank-csv`, configure the existing confirmed `bank` account role and exactly
one active bank account mapped to that asset; all statement currencies must
match it. Missing/ambiguous mapping fails with an actionable report, retaining
already completed intake. For several bank accounts, use the existing bank
import with an explicit account first, then run AgentRun without `--bank-csv`.
All physical-source, current-authority, VAT, amount, period and duplicate guards
remain in the common writers. Each effect commits separately: an error is not
an all-run rollback, and unknown/retired responses are never automatically retried.

The full ordinary CLI/server/shared-React synthetic proof at90aef554 produced
five journals/fourteen lines: three explicit payable registrations, one automatic
expense and one creditor settlement. Four bank rows/two links, one payment,
actual-principal agent attribution, native proposals visible in the existing
cockpit, duplicate no-reposting, pre-ingest bad-sidecar refusal, retained intake
after bank failure and reviewer refusal were independently checked. Native
fixtures also cover service identity/revocation, physical tamper, VAT/fixed-asset
refusal, exact phase/report shape and unknown outcomes. The current catalogue
is1615operations (651HTTP/532CLI/432MCP); the previous1613business contracts and
schemas are preserved except source pins. No standalone MCP `agent_run` existed
in the source or is invented here.

This accepts this original user flow only. Full716, remaining original commands,
all55/J01–J07, cutover and final installed desktop acceptance remain open. The
legacy default entrypoint and live company workspaces are unchanged.
