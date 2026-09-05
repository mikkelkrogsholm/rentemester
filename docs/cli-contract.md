# CLI-kontrakt: actor-politik og exit-koder

Dette dokument beskriver to forudsætninger, som ikke fremgår af den enkelte
kommandos `--help`, men som en agent skal kende for at kalde `rentemester`-CLI'en
korrekt. Implementeringen ligger i `src/cli-actor.ts` og `src/cli.ts`.

## 1. Actor-politik for muterende kommandoer

`legacy-party-mapping plan` er read-only. `legacy-party-mapping apply` kræver
`--confirm yes`, actor, en autentificeret workspace-servicekonto, idempotency
key, eksakt plan-hash og et eksisterende kildebilag; den er idempotent. Planen
binder bilagets hash og den gennemgåede reference. En reviewed legacy reference
alene opretter aldrig en mapping.
Korrektioner sker kun med `legacy-party-mapping supersede`; kontakt, bilag,
journal og moms ændres ikke.

En importeret leverandør med `identity_status=human_resolution_required` og
manglende `country_code`/`identifier_kind` klargøres først med
`vendor-identity-enrichment plan` og `apply`. Planen verificerer det registrerede
originalbilags faktiske bytes og binder leverandørens nuværende navn, adresse og
eventuelle eksisterende identifikator. `apply` kræver samme service-principal,
actor-, confirmation-, plan-hash- og idempotency-gates som legacy mapping og
udfylder kun de manglende typefelter. Den opfinder ikke et ID og ændrer ikke
leverandørens ID, navn, adresse, noter, bilaget, journalen eller momsdata.

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

### TastSelv momsangivelse

`vat momsangivelse` / `vat filing` og MCP `vat_filing` returnerer samme
read-only form: `salgsmoms`, `kobsmoms`, `momsAfVarekobUdland`,
`momsAfYdelseskobUdland`, A-varer, A-ydelser, de tre B-felter, C og de seks
afgiftsrefusionsfelter samt `momsIAlt`. Alle indtastningsfelter er signed
integer DKK. Hvert råbeløb trunceres mod nul før `momsIAlt` beregnes; rå
ledger- og momsrapportværdier bevarer øre. Tvetydigt B-salg eller en
refusion uden append-only evidence afviser filing-rapporten. Ingen kommando
indsender til Skattestyrelsen.

De primære, versionsbundne kilder er [Skattestyrelsens vejledning om angivelse i hele kroner](https://info.skat.dk/data.aspx?oid=2062862) og [TastSelv-dataformatets feltdefinitioner](https://info.skat.dk/data.aspx?oid=1878548). Den lovbundne kildehash står i `DK-VAT-FILING-001` i `rules/dk/vat.yaml`; nye feltfortolkninger må ikke indføres uden fornyet kildekontrol.

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
