# CLI-kontrakt: actor-politik og exit-koder

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
actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/OPENCLAW_AGENT set
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

Alle andre muterende kommandoer (faktura-bogføring, journal-postering,
bank-import, periode-luk, …) kræver **ikke** `--confirm yes` — `--actor`
er kontrakten. Det modsatte gælder for samme operation på MCP (alle writes
kræver `confirm: true`); afvigelsen er bevidst og forklaret i
[`docs/confirm-contract.md`](confirm-contract.md).

## 4. Output-felter ved succes

Den enkelte kommandos `--help` dækker exit-koder og — ved fejl — `errors[]`,
men *ikke* hvilke felter `--json`-succes-outputtet indeholder. Den kontrakt
står ikke i `--help`.

Reglen er: **et `--json`-succes-output fra CLI'en spejler `data`-shapen for
den tilsvarende MCP-tool.** Hver CLI-kommando svarer (typisk 1:1) til en
MCP-tool — `journal post` ⇄ `journal_post`, `invoice issue` ⇄ `invoice_issue`,
`audit verify` ⇄ `audit_verify` osv. — og de to overflader returnerer den
samme strukturerede `{ ok, errors, ... }`-form med de samme felter under
resultatet.

Den autoritative, per-tool feltliste står derfor i
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md) under afsnittet
"`data`-felter pr. tool" (samt i kildens `*Result`-typer i `src/core/*.ts`).
Slå CLI-kommandoens MCP-pendant op dér for at se de præcise felter — fx giver
`audit verify` / `audit_verify` `{ entries }` med integritets-verdikten i
`ok`/`errors[]`, og `journal post` / `journal_post` giver
`{ entryId, entryNo, entryHash }`.

Bemærk de få CLI-only-kommandoer uden MCP-pendant (fx `invoice create`,
`invoice export-public`); de er listet under "CLI/MCP-mapping" i samme
dokument.
