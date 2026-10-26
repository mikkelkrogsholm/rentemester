# CLI-kontrakt: actor-politik og exit-koder

## Rust-klienten og agentopslag

### Manuelle opgaver gennem Rust-serveren

`rentemester-rust tasks list|get|history|receipt|create|update|move|complete|reopen`
bruger den fælles server med `--server ORIGIN` eller den eksisterende
`--local-profile`, og præcis én privat `--cookie-file` eller `--service-key-file`.
`tasks --help` viser de konkrete JSON-inputskemaer. Input læses fra `--input FILE`
eller stdin; `list` tillader et tomt input som `{}`. Muterende operationer kræver
`--confirm`, callerens `idempotencyKey` og ved eksisterende opgaver en positiv
`expectedVersion`. Actor og principal udledes på serveren; `--actor`, `local`
og autorisationsfelter i JSON accepteres ikke.

Eksempel på læsning: `tasks get --server ORIGIN --cookie-file PRIVATE_COOKIE`
med stdin `{"taskId":"task-reference"}`. Ved ukendt skriveudfald beholdes det
oprindelige input og nøglen; `tasks receipt` med `{"idempotencyKey":"original-key"}`
viser kun en kvittering, som stadig er synlig med de aktuelle rettigheder.
Kontrollér også `get`/`history`, før en ny handling startes. Klienten gentager
aldrig automatisk en skrivning. Historiske ansvarlige eller receipts giver ingen
adgang. Beskyttede task-linked knowledge-forløb er endnu ikke porteret.

### Påmindelser og live runtime gennem Rust-serveren

`tasks reminder-set` / MCP `tasks_reminder_set` kræver `taskId`, den præcise
`reminder`, positiv `expectedVersion`, callerens `idempotencyKey` og bekræftelse.
`tasks notifications` / `tasks_notifications` og `tasks runtime` / `tasks_runtime`
er reads uden inputfelter. De viser kun aktuelt autoriserede påmindelser og en
faktisk registreret worker; et gemt heartbeat alene betyder ikke running.

`tasks run` / `tasks_run` er et eksplicit bekræftet tick for workspace owner med
`companySlugs?`, `asOfDate?`, `idempotencyKey`. Det starter ingen baggrundsproces.
Aktuel adgang til både kildescope og det historiske notificationscope kontrolleres
før receipt-replay. Ingen brugeridentitet kan komme fra input.

Kontinuerlig kørsel kræver serverens parrede `--task-worker
--task-service-key-stdin --config PRIVATE.json` ved almindelig genåbning.
Nøglen læses én gang fra stdin og holdes kun i hukommelsen. Den eksisterende
servicekonto skal have aktuelle task-rettigheder; ingen local/System-grant,
gemt human cookie eller automatisk oprettet konto. Kørsel hvert 60. sekund bruger
en holdbar 55-sekunders lease og præcis én servicecharge pr. vedligeholdelsestick.
Kilder, rutiner og levering committes atomisk. Revokeret credential/recipient
eller mistet scope stopper virkningen; disabled/done-planer vises ikke.

Engangs- og daglige slots bruger modtagerens IANA-tidszone og nuværende dag uden
backlogburst. Genstart gentager ikke et leveret slot. Task-business-codec5 bevarer
notification-slots og brugbare reminder/run-receipts; gamle codec1–4 beholder egne
bytes/kontroller. Runtimelease, heartbeat, fejlmetadata, keys og adgangsgrants
aktiveres aldrig ved recovery. Kanalen er kun Rentemesters egen brugerflade.

### Autoritative opgavekilder gennem Rust-serveren

`rentemester-rust tasks sync` og MCP `tasks_sync` tager `companySlugs?`,
`asOfDate?` og callerens `idempotencyKey`; skrivningen kræver eksplicit
bekræftelse. Udeladt scope betyder alle aktuelt skrivbare Live-selskaber;
`[]` betyder ingen og kræver workspace owner. Historisk og aktuel adgang
kontrolleres før receipt-replay og før nogen ledger åbnes.

Kilder læses fra de native bank-, undtagelses-, periode-, moms- og
batchkontroller med verificerede originaler. Alle workbench-sider gennemgås;
utilgængelige kilder bevares som uafklarede med selskabets fejl i `sync.errors`.
En periode, der blot er beregnet eller lukket, er ikke en momsindberetning.
Produktverificeret afslutning kræver det faktiske native resultat og evidens;
indberetningsstatus kræver den registrerede reference. Årsrapportpligt kræver
stadig eksplicit relevansafklaring. Genåbning bevarer dokumentationshistorikken.

Sync gemmer atomiske task-events og originale receipts uden at bogføre,
betale eller indberette eksternt. Task-business-codec4 bevarer kildesnapshots,
no-op receipts og scoped summer i kildefri backup/restore; ældre manual1,
rutine2 og board3 beholder deres egne bytes og kontroller. Dette starter
ikke nogen automatisk runner.

### Rutiner gennem Rust-serveren

`rentemester-rust task-series list|save|project|materialize` bruger samme transport
og private credentials. MCP-navnene er `task_series_list`, `task_series_save`,
`task_series_project` og `task_series_materialize`; hjælp og skemaer viser de
præcise flade JSON-input. `save` kræver `series`, `expectedVersion` (`0` ved
oprettelse) og `idempotencyKey`. `materialize` accepterer `companySlugs?`,
`asOfDate?` og `idempotencyKey`. Begge skrivninger kræver eksplicit bekræftelse.
`project` kræver `from` og `to`; projektion opretter ingen opgaver.

Hvert berørt selskab kræver aktuel manage-adgang før en gemt effekt kan
returneres. Tomt workspace-scope kræver workspace owner; en eksplicit tom
`companySlugs` bliver ikke til alle selskaber. Fremtidige revisioner og pause
ændrer ikke allerede konkrete perioder. `tasks receipt` kan også kontrollere
de oprindelige rutinenøgler. Materialisering er eksplicit; denne leverance
starter ingen automatisk runner.

### Brugerdefinerede opgaveboards gennem Rust-serveren

`rentemester-rust task-boards list|preview|save` og MCP `task_boards_list`,
`task_boards_preview`, `task_boards_save` bruger samme private servertransport.
`preview` tager `board` og valgfri `expectedVersion`; den læser kun og afviser
bekræftelse og skrivefelter. `save` kræver `board`, `expectedVersion` (`0` ved
oprettelse), callerens `idempotencyKey` og eksplicit bekræftelse. Hjælp viser
de oprindelige typede JSON-skemaer.

Hvert statusområde skal have præcis én standardkolonne. Fjernes en befolket
kolonne, kræves en eksplicit relocation. Ændret statusbetydning kræver det
aktuelle `previewHash`; ændringer kan aldrig omgå dokumenteret afslutning
eller genåbning. Strukturelle workspaceændringer kræver aktuel adgang til
alle berørte selskaber, uanset om deres opgaver er synlige. Opgave-, board-
og receipt-events gemmes atomisk. Et identisk gemt resultat genbruger sin
oprindelige version efter aktuel adgangskontrol, også efter senere kolonner.

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

`documents party-link-apply`, `party-link-supersede`,
`internal-no-external-party` og `internal-no-external-party-supersede` kræver
`RENTEMESTER_SERVICE_PRINCIPAL_TOKEN` og `RENTEMESTER_WORKSPACE` i processen.
Den aktive token og virksomhedens aktuelle `company.master-data`-membership
verificeres før mutation. Gemt principal er `service-account:<serviceAccountId>`
fra autentificeringen; `--actor` forbliver den separate policy-godkendte
revisionsidentitet. Udelad normalt `--principal`: flaget er kun en valgfri
lighedsassertion og afvises, hvis det ikke matcher den verificerede principal.
MCP's tilsvarende dokument-part-operationer udleder også principal fra den
autentificerede servicekonto, aldrig fra klientens actor eller inputfelter.
Eksisterende historik omskrives ikke.

Efter en godkendt legacy mapping bruges de samme identitets-/referencefelter
fra dokument-part-planen ved apply, sammen med dens eksakte `--plan-hash`,
`--idempotency-key`, `--confirm yes` og `--actor`. CLI's `--company` er her
virksomhedsstien; `--workspace` og `--company-slug` skal pege på samme
autentificerede workspace og virksomhed. Se `docs/deployment-modes.md` for
lokal service-bootstrap og sikker tokenoverførsel til containerprocessen.

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
3. En **udledt** actor fra miljøet, hvis intet eksplicit er sat (samme
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

Workspace-kommandoerne `company-knowledge propose/review/supersede` og
`ownership propose/review/apply` kræver aktive, registrerede og initialiserede
virksomheder. Actoren skal stå i hver berørt virksomheds allowlist, også når
actoren er udledt fra miljøet. Knowledge review/supersede bruger virksomhedens
slug fra den gemte assertion; ownership review/apply bruger alle endpoints fra
det gemte snapshot. Afvisning sker før workspace-kontrolbasen åbnes til skrivning.

Bankafstemning for fakturabetaling, refundering og kravbetaling kræver en
entydig `bankTransactionReference`. Ved flere rækker med samme reference skal
`bankTransactionId` angives. Angives begge, skal referencen tilhøre det valgte ID.
En afvisning bogfører eller afstemmer intet.

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

### Original `report analytics` on the Rust runtime

`report analytics --workspace ROOT_OR_ID --scope company|portfolio|group
--from DATE --to DATE --local-profile ABSOLUTE_PRIVATE_PROFILE --cookie-file FILE`
uses the existing authoritative CFO producer. Original comma-list selectors and
`{ok:true,analytics}` output are retained; money strings, provenance and partial or
blocked states are preserved. Human success is `✔ report-analytics`. The workspace
assertion must exactly match the verified local profile root or ID. Original remote
workspace assertions remain unavailable; `remote cfo-analytics` stays remote-capable.
No local ledger reads, inferred workspace, client calculations or automatic retries.
This closes one command gap in #716; full CLI parity and cutover remain open.

### Standardlauncher og eksplicit historisk CLI

`bin/rentemester` starter nu `rentemester-rust` (pakket søskendebinær eller PATH).
`RENTEMESTER_RUST_CLI=/absolut/sti/rentemester-rust` vælger et eksplicit Rust-artefakt.
Launcher bygger ikke og falder aldrig automatisk tilbage til TypeScript ved fejl.
Kun det eksplicitte historiske kald `RENTEMESTER_LEGACY_CLI=1 bin/rentemester …`
benytter Bun og `src/cli.ts`; det er ikke en Rust-fejlgendannelse eller migration.
Private operatorhandlinger anvender samme Rust-server og Owner, aldrig en ekstra
motor. `--actor` er kun en assertion mod aktuelle credentials; schema/bootstrap
kræver den private serverkonfigurations tilladte operator. Paths giver ikke adgang.

### Original #716: operator-, backup- og overførselsprofiler

De 265 aktuelle originale kommandoidentiteter findes i
`crates/rentemester-engine/fixtures/current-cli716.json`. Dette er kildemetadata,
ikke en tilladelse eller en påstand om fuld accept. De konkrete input-, output-,
fejl- og renderingkontrakter findes i de 38 `api-contracts/`-partitioner.

`init`, `serve`, `local start` og `workspace-access bootstrap-first` kalder det
installerede søskendeprogram `rentemester-server` én gang; en eksplicit absolut
`RENTEMESTER_SERVER_BIN` kan vælge et eksisterende program. De starter ikke Bun,
Cargo eller en anden motor. Bootstrap bruger den private operatorpolitik og en
privat password-fil; caller actor/path erstatter ikke denne politik. Schema
`system migrate|repair-schema-views` inspicerer en beskyttet kopi uden mutation;
kun `--apply yes` med eksplicit `--server-bin` og `--config` bruger den stoppede
fælles servers namespace-lease. Der findes ingen HTTP-schema-write-genvej.

`workspace snapshot|restore` kræver eksplicit `--config` og absolut
`--server-bin`; transporten er en enkelt privat proces, ikke en ny HTTP-route.
Standardformatet `--artifact-format source-tar-v1` er den originale TAR-profil
med manifest, oprindelige selskabsfiler og separat adgangsgenopretningsplan.
Snapshot kræver aktuelle cookie-credentials og tilladt operator; restore kræver
uafhængigt betroede selskabsnøgler (`--trust-keys`), frisk eksakt konfigureret
mål og `--source-unavailable yes`. Indlejrede nøgler giver ikke tillid.
`native-v1` er et **eksplicit andet format**, `rentemester-native-backup/1`,
med `rentemester-cli-workspace/2`-receipt. Det bruger konfigureret signing key
og `--trusted-backup-key`, aldrig implicit TAR-konvertering eller fallback.
`--at` og `--trust-keys` tilhører kun source-TAR-profilen. Ikke-understøttede
historiske schemas eller ikke-understøttede forretningssektioner kræver en
separat eksplicit versioneret migration; de må ikke droppes for at få restore
igennem. Restore genskaber ikke sessioner eller provider-credentials: næste trin
er `bootstrap-owner-then-reinvite`.

Originale company-backups/HMAC/Ed25519 og native whole-workspace-signaturer er
forskellige kompatibilitetsprofiler. Offsite-attestation er ikke verificeret
placering, provider-approval eller en udført recovery. Ingen automatisk retry ved
usikkert svar, delvis filpublicering eller ukendt commit; behold receipt, snapshot,
sidecar og privat stage til afstemning. Remote paths giver aldrig lokal adgang.

De beskyttede namespace-, custody- og restoreprofiler kræver de faktisk
kvalificerede Unix/Linux-miljøer og korrekt UID/filmode/descriptor-admission.
En syntetisk macOS-test er ikke bevis for Linux broker/cgroup eller et ægte
provider-/hosted-miljø. Et ikke-kvalificeret miljø afvises eksplicit; der findes
ingen skjult TS-fallback. Ingen live migration eller cutover er accepteret her.

## Historical TypeScript task source reference (not native task parity)

The following describes the explicitly selected legacy source implementation. Native task port and schema adoption remain separate work; do not apply these local-access or retry rules to native clients.

## 5. Opgaver, rutineserier og boards

`tasks`, `task-series` og `task-boards` bruger det eksplicit valgte
`--workspace <dir>` (eller `RENTEMESTER_WORKSPACE`). De kræver aldrig
`--company` eller en standardvirksomhed og åbner ikke en ledger til skrivning.
Den fælles opgaveservice filtrerer adgang før søgning og optælling og kontrollerer
actor-allowlisten for alle berørte selskaber. For arbejde uden selskaber bruges
`<workspace>/config/policy.yaml` med samme `actor_allowlist`-format. Hvis den
workspace-policy mangler, kræves en actor tilladt i mindst ét registreret
selskab; uden nogen sådan policy afvises mutation. Tildeling ændrer ingen
rettigheder. `ACTOR_REQUIRED` og `ACTOR_DENIED` er CLI-brugsfejl (exit `2`).

CLI med `RENTEMESTER_SERVICE_PRINCIPAL_TOKEN` bruger tokenens verificerede
servicekonto og aktuelle membership. Den valgte workspace skal være den
samme som den autentificerede workspace. En ugyldig token kan aldrig falde
tilbage til lokal adgang. Uden servicecredentials er CLI et betroet lokalt
filsystemværktøj. `--actor` er separat revisionsidentitet og giver ingen adgang.

Mutationer kræver `--input <file.json>` med et typet JSON-objekt og
`idempotencyKey`. Eksisterende opgaver kræver også `taskId` og
`expectedVersion`; ændringer af en serie eller et board kræver den aktuelle
version, eller `0` ved oprettelse. Et identisk genforsøg returnerer den gemte
effekt; ændret payload med samme nøgle afvises. Brug en ny nøgle til en ny
synkronisering eller reminderkørsel. Et uklart resultat verificeres med `tasks
get` og `tasks history`, før samme request genforsøges.

Læsning accepterer enten `--input` eller de dokumenterede selector-/filterflags,
aldrig begge. `--companies` er en kommasepareret slugliste. `--include-archived` er et bart booleanflag. Værdiflags
`--show-done`, `--undated` og `--unassigned` accepterer kun `true` eller `false`. Alle datoer er `YYYY-MM-DD`.

| CLI | MCP | JSON-input ud over workspace |
| --- | --- | --- |
| `tasks list` | `tasks_list` | `companySlugs?`, `includeArchived?`, `status?`, `type?`, `search?`, `assigneeId?`, `from?`, `to?`, `undated?`, `unassigned?`, `showDone?` |
| `tasks get`, `tasks history` | `tasks_get`, `tasks_history` | `taskId` |
| `tasks create` | `tasks_create` | `title`, eksplicit `scope`, `idempotencyKey` og valgfrie opgavefelter |
| `tasks update` | `tasks_update` | `taskId`, `patch`, `expectedVersion`, `idempotencyKey` |
| `tasks move` | `tasks_move` | `taskId`, fælles `status`, `columnId?`, `workspaceColumnId?`, `expectedVersion`, `idempotencyKey` |
| `tasks complete` | `tasks_complete` | `taskId`, `outcome`, `note`, `references?`, `expectedVersion`, `idempotencyKey` |
| `tasks reopen` | `tasks_reopen` | `taskId`, `reason`, `expectedVersion`, `idempotencyKey` |
| `tasks sync` | `tasks_sync` | `companySlugs?`, `asOfDate?`, `idempotencyKey` |
| `tasks reminder` | `tasks_reminder_set` | `taskId`, `reminder`, `expectedVersion`, `idempotencyKey` |
| `tasks notifications`, `tasks runtime` | `tasks_notifications`, `tasks_runtime` | Ingen forretningsfelter |
| `tasks run` | `tasks_run` | `companySlugs?`, `asOfDate?`, `idempotencyKey` |
| `task-series list` | `task_series_list` | `companySlugs?`, `includeArchived?` |
| `task-series save` | `task_series_save` | `series`, `expectedVersion`, `idempotencyKey` |
| `task-series project` | `task_series_project` | `from`, `to`, `companySlugs?`, `includeArchived?` |
| `task-series materialize` | `task_series_materialize` | `companySlugs?`, `asOfDate?`, `idempotencyKey` |
| `task-boards list` | `task_boards_list` | `companySlugs?`, `includeArchived?` |
| `task-boards preview` | `task_boards_preview` | `board`, `expectedVersion?` |
| `task-boards save` | `task_boards_save` | `board`, `expectedVersion`, `idempotencyKey` |

`scope` er `{kind:"company",companySlug:"syntetisk-drift"}` eller
`{kind:"workspace",companySlugs:[...]}`. Fælles status er `open`, `in_progress`,
`waiting` eller `done`. `outcome` er `completed`, `not_relevant`, `cancelled`
eller `exception`; et særskilt outcome forhindrer, at en begrundet undtagelse
fremstår som en faktisk indberetning. Flytning til `done` omgår aldrig
beviskrav eller kildekontrol.

Den præcise typede feltliste for `patch`, `series`, `board` og `reminder`
fremgår af CLI'ens per-kommando `--help` og MCP's operation description/schema.
Et minimalt manuelt opgaveinput er:

```json
{"title":"Kontrollér månedens bilag","scope":{"kind":"company","companySlug":"syntetisk-drift"},"idempotencyKey":"manual-task-1"}
```

Alle opgavekommandoer, også reads, returnerer flad
`{ok:true,errors:[],...serviceFields}` på CLI. MCP wrapper de samme servicefelter
under `data`. Dette er en dokumenteret undtagelse til de ældre read-former i
§4. `TaskError` bevarer sin stabile `code`; businessfejl er exit `1`, ugyldige
flags/JSON/schemafelter er exit `2`.

CLI's almindelige opgavewrites kræver actor. `task-boards save` kræver desuden
altid `--confirm yes`; ændring af betydning eller sletning af en fyldt kolonne
kræver den aktuelle `previewHash` og eksplicit omplacering. MCP kræver
`confirm:true` ved **alle** opgavewrites. Hver opgaveændring og dens historik
committes atomisk i workspacebasen, uden bogføring, betaling eller indberetning.

`tasks run` kører ét deterministisk tick og afslutter; det starter ikke en
baggrundsproces. Påmindelser er kun interne og brugerens aktivering er
udtrykkelig. `tasks runtime` viser den observerede runnerstatus.
