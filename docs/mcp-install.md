# Tilslut den native Rust MCP-klient

Rentemesters native MCP-proces er en stdio-klient til den fælles Rust-server.
Den starter ingen ekstra bogføringsmotor. Brug en absolut sti til den
identificerede native `rentemester-mcp`-binær. Repositoryets
`bin/rentemester-mcp` er fortsat en Bun-wrapper; den endelige udfasning i #727
venter på de oprindelige release-/cutovergates.

### Historical TypeScript source profile

Rentemester eksponerer sin CLI som en MCP-server, så agenter (Claude
Desktop, Cursor, Claude Code, Codex m.fl.) kan kalde
bogføringskommandoer direkte. Serveren eksponerer hele tool-surface'en —
**269 tools** — fordelt på domænerne `invoice_*`, `bank_*`, `journal_*`,
`vat_*`, `system_backup_*`, `asset_*`, `mileage_*`, `recurring_invoice_*`
m.fl. Den autoritative liste med klassifikation og inputs står i
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md); kør `tools/list` mod en
kørende server for den faktiske, aktuelle liste.

## Kontrollér binær og katalog

```sh
/absolute/native/rentemester-mcp --help
/absolute/native/rentemester-mcp --catalogue
```

Begge er offline-opslag uden virksomhedsadgang. `--catalogue` skal stå alene;
svaret indeholder protokolversion og den faktiske `tools`-liste med skemaer og
annotations. Efter forbindelse bruges `tools/list` og `meta_about` til den
kørende proces. Senest accepterede registry har 448 tools, men antal og
skemaer skal altid hentes fra den valgte revision. De tidligere 214 tools er
en historisk TypeScript-reference, ikke installationskontrakten.

## Forbind til en HTTPS-server

En menneskelig bruger logger ind gennem det almindelige password/MFA-forløb.
CLI-login kan skrive en ny privat sessionsfil med `--session-out`; det kræver
privat stdin uden terminalekko og overskriver ikke en eksisterende fil. Se
[loginvejledningen](rust-server.md#cli-login-med-adgangskode-og-mfa).
MCP læser Cookie-headerens værdi fra en privat regulær fil, på Unix `0600`.
Skriv aldrig selve cookien i argumenter, klient-JSON, miljøvariabler eller logs.

En MCP-klient med `mcpServers`-konfiguration kan fx bruge:

```json
{
  "mcpServers": {
    "rentemester": {
      "command": "/absolute/native/rentemester-mcp",
      "args": [
        "--server", "https://accounting.example.invalid",
        "--cookie-file", "/absolute/private/session.cookie"
      ]
    }
  }
}
```

Tilpas adressen og de absolutte stier lokalt. Indsæt samme serverdefinition i
den valgte klients MCP-indstillinger; klientens placering af konfigurationen
kan variere. Genstart forbindelsen efter ændringer. Bekræft initialize,
`tools/list`, `meta_about` og et autoriseret read-only virksomhedsopslag før
nogen mutation. Udskriv ikke credential-filen for at fejlsøge forbindelsen.

HTTPS er standard. `--allow-loopback-http` tillader kun en eksplicit literal
loopbackadresse ved lokal afprøvning; det er ikke en undtagelse for en vilkårlig
HTTP-server. Klienten følger ikke redirects eller skifter automatisk backend.

## Tilslut en allerede kørende lokal server

```sh
/absolute/native/rentemester-mcp \
  --local-profile /absolute/private/local-profile \
  --cookie-file /absolute/private/session.cookie
```

Profilen skal være den aktuelle beskyttede profil med matchende kanonisk
descriptor og privat Unix-socket fra den administrerede server. Dette starter
ikke serveren og giver ingen myndighed ved mappe- eller actor-navn.
`--local-profile` kan ikke kombineres med `--server` eller
`--allow-loopback-http`. Ved ny logisk origin skal MCP stoppes og
konfigureres/genstartes eksplicit; der er ingen TCP- eller child-serverfallback.

## Servicekonto og attribution

En ejer opretter og aktiverer servicekontoen gennem **Brugere og adgang** og
giver den nødvendige virksomhedsrolle. Vælg `--service-key-file PRIVATE_PATH`
i stedet for `--cookie-file`; de må aldrig kombineres. Nøglen kræver en privat
regulær fil og må ikke indsættes i klientargumenter eller logs.

Den aktuelle server kontrollerer adgang og tilbagekaldelse på hver operation.
Værktøjets navn eller tilstedeværelse i kataloget lover ikke, at en servicekonto
må bruge det; flere ejer-/administrationsforløb kræver menneskelig MFA.
Forbrug og kvitteringsreplay følger den aktuelle operation, også ved et
udtrykkeligt nyt servicekald.

Autoritet kommer fra den autentificerede bruger/serviceidentitet.
`RENTEMESTER_MCP_USER`, et handshake-navn eller en påstået actor er ikke login
eller en adgangstildeling i Rust. Brug aktuelle skemaer og serverens kanoniske
attribution; overfør ikke legacy-miljøbaserede rettighedsantagelser.

## Arbejd gennem discovery og gennemgang

Begynd med `meta_about {}`, søg et konkret resultat med
`agent_capability_search`, og kald `agent_workflow_describe` med det returnerede
`id`. Følg workflowets læsning, plan, gennemgang, godkendelse og apply samt det
aktuelle tools inputskema. Metadata giver ingen bogføringsmyndighed.

Eksempel på et read-only discovery-kald efter MCP-initialize:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": { "name": "meta_about", "arguments": {} }
}
```

Brug derefter en eksplicit autoriseret virksomhed fra serverens synlige
virksomheder; en lokal sti vælger ikke skjult myndighed. Writes kræver den
præcise operations bekræftelse, ofte både ydre `confirm:true` og en konkret
reviewet body. Der er ingen universel payload eller generel retry-garanti.
Gem den oprindelige nøgle/envelope, og genlæs kvittering/journal/status ved
ukendt udfald. En ny nøgle er ikke en sikker gentagelse.

Læs [agentkontrakten](mcp-agent-contract.md) og
[ejer-/operatørhjælpen](rust-support.md). Ved forbindelsesfejl kontrolleres
binærsti, serverorigin, privat filtilstand, aktuel session/rolle og lokal
profilbinding. Ved mistet svar afklares den eksisterende handling; opret ikke
et testbilag eller en testpostering i den rigtige virksomhed for at kontrollere
forbindelsen.
