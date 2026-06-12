# Rentemester MCP-server — installation

Rentemester eksponerer sin CLI som en MCP-server, så agenter (Claude
Desktop, Cursor, Claude Code, Codex m.fl.) kan kalde
bogføringskommandoer direkte. Serveren eksponerer hele tool-surface'en —
**101 tools** — fordelt på domænerne `invoice_*`, `bank_*`, `journal_*`,
`vat_*`, `system_backup_*`, `asset_*`, `mileage_*`, `recurring_invoice_*`
m.fl. Den autoritative liste med klassifikation og inputs står i
[`docs/mcp-tool-surface.md`](mcp-tool-surface.md); kør `tools/list` mod en
kørende server for den faktiske, aktuelle liste.

`initialize`-svaret indeholder også en kort `instructions`-streng der
orienterer agenten om rækkefølge og confirm/destructive-konventioner. Den
fulde kontrakt for den løse tool-surface står i
[`docs/mcp-agent-contract.md`](mcp-agent-contract.md).

## Installer Rentemester globalt

Forudsætning: [Bun](https://bun.sh) >= 1.2.

```bash
git clone https://github.com/mikkelkrogsholm/rentemester.git
cd rentemester
bun install
bun link            # registrerer både `rentemester` og `rentemester-mcp`
                    # som globale shims
```

Verificér at MCP-binaryen kan findes:

```bash
which rentemester-mcp
rentemester-mcp --help    # (serveren accepterer alle args men kører
                           # stdio-server uanset)
```

## Claude Desktop

Rediger `~/Library/Application Support/Claude/claude_desktop_config.json`
(macOS) eller `%APPDATA%\Claude\claude_desktop_config.json` (Windows) og
tilføj Rentemester under `mcpServers`:

```json
{
  "mcpServers": {
    "rentemester": {
      "command": "rentemester-mcp",
      "args": [],
      "env": {
        "RENTEMESTER_MCP_USER": "mikkel@56n.dk"
      }
    }
  }
}
```

Genstart Claude Desktop. I en ny chat skal "rentemester" nu fremgå af
listen over MCP-servere, og `audit_verify` + `journal_post` skal være
kaldbare.

### Hvis `rentemester-mcp` ikke ligger i PATH

Brug en absolut sti og `bun` direkte:

```json
{
  "mcpServers": {
    "rentemester": {
      "command": "/Users/mikkel/.bun/bin/bun",
      "args": ["/Users/mikkel/code/rentemester/src/mcp/server.ts"],
      "env": {
        "RENTEMESTER_MCP_USER": "mikkel@56n.dk"
      }
    }
  }
}
```

## Cursor

Cursor's MCP-konfiguration ligger i `~/.cursor/mcp.json`. Samme format
som Claude Desktop:

```json
{
  "mcpServers": {
    "rentemester": {
      "command": "rentemester-mcp",
      "args": []
    }
  }
}
```

## Claude Code

Tilføj serveren via `claude mcp add`:

```bash
claude mcp add rentemester --command rentemester-mcp
```

Eller manuelt i `~/.claude.json` under `mcpServers` med samme JSON-form
som ovenfor.

## Actor-attribution

Hvert MCP-call tilskrives som `agent:<client-name>/<client-version>`,
fx `agent:claude-desktop/0.7.6`. Hvis du sætter `RENTEMESTER_MCP_USER`
i env, vises den i `created_by_program` på journal-posteringer og i
`audit_log.actor` som:

```
agent:claude-desktop/0.7.6 via mcp:mikkel@56n.dk
```

Dette er den traceable kæde fra agent-call til append-only-bogføring.
Aktor-info udledes per request fra MCP-klientens handshake og passes
som **eksplicit parameter** til kernen — ikke som proces-env-var — så
flere parallelle calls ikke kan løbe ind i hinanden.

## Eksempel-tool-call (rå MCP JSON-RPC)

`audit_verify`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "audit_verify",
    "arguments": { "company": "/Users/mikkel/companies/acme-aps" }
  }
}
```

Respons:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{"type": "text", "text": "{\"ok\":true,\"data\":{\"entries\":142},\"errors\":[]}"}],
    "structuredContent": { "ok": true, "data": { "entries": 142 }, "errors": [] },
    "isError": false
  }
}
```

`journal_post` (kræver `confirm: true`):

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "journal_post",
    "arguments": {
      "company": "/Users/mikkel/companies/acme-aps",
      "payload": {
        "transactionDate": "2026-05-18",
        "text": "Manuel postering — kontorartikler",
        "documentId": 12,
        "lines": [
          { "accountNo": "3000", "debitAmount": 320.00, "vatCode": "DK_PURCHASE_25" },
          { "accountNo": "1200", "debitAmount": 80.00 },
          { "accountNo": "2000", "creditAmount": 400.00 }
        ]
      },
      "confirm": true
    }
  }
}
```

## Fejlsøgning

- **Claude Desktop viser ikke serveren.** Tjek at JSON er valid, og at
  `command` peger på en binary i PATH eller en absolut sti. Genstart
  Claude Desktop helt (Cmd+Q) — det er ikke nok at lukke vinduet.
- **`tools/call` returnerer `confirm: true required`.** Write-tools
  kræver eksplicit `confirm: true` på input. Det er bevidst — det
  forhindrer agenter i at bogføre ved et uheld.
- **`company path does not exist`.** MCP-serveren accepterer aldrig
  implicit "current company"; agenten skal altid passe en `company` —
  enten en absolut sti til virksomhedsmappen eller en workspace-slug
  (slås op i `RENTEMESTER_WORKSPACE`'s manifest).
  Kør `rentemester init --company /path/to/company` først.
