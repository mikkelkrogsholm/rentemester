# Rentemester agent-demo

End-to-end loop der demonstrerer Rentemesters thesis:

> **Agenten handler, reglerne afgør, ledgeren håndhæver.**

Du kører ét script. Det spinner en frisk MCP-server op, importerer en
måneds bilagsmail, foreslår match mod bankudtog, auto-bogfører høj-
confidence matches, og lader resten ligge i en exception-kø som et
menneske skal kvittere.

Ingen API-keys, ingen netværk. Alt kører lokalt over JSON-RPC mod
`src/mcp/server.ts`.

## Kør den

```
bun examples/agent-demo/run.ts --company /tmp/agent-demo --mode rule-based
```

Tilgængelige flag:

| Flag | Default | Betydning |
|------|---------|-----------|
| `--company` | (kræves) | Sti til ny virksomhedsmappe. **Slettes hvis den findes.** |
| `--mode` | `rule-based` | `rule-based` (deterministisk) eller `claude` (Anthropic API hvis `ANTHROPIC_API_KEY` er sat — ellers fallback til rule-based). |
| `--demo-dir` | `examples/agent-demo/` | Hvor inbox/metadata/bank.csv ligger. |

Kortere: `bun run agent-demo --company /tmp/agent-demo`.

## Hvad demoen indeholder

```
examples/agent-demo/
├── inbox/                          5 vendor-fakturaer + 1 cash-register-bon
│   ├── google-workspace-2026-05.txt
│   ├── openai-2026-05.txt
│   ├── aws-hosting-2026-05.txt
│   ├── dsb-rejse-2026-05.txt
│   ├── elgiganten-hardware-2026-05.txt
│   └── restaurant-2026-05.txt      ← ender i exception-kø (cash-register
│                                      receipt uden formålsbeskrivelse —
│                                      mangler det human-only formål-felt der
│                                      gør det fradragsberettiget)
├── metadata/                       parallel JSON med leverandøroplysninger
└── bank.csv                        7 bank-transaktioner der dækker alle
                                    udgifter + Stripe-payout uden kunde-faktura
```

Mock-data er anonymiseret/syntetisk. Alle leverandørnavne er rigtige
virksomheder, men beløb, fakturanumre, VAT-IDs og betalingsmønster er
opfundet til demoen.

## Forventet output

```
Rentemester agent-demo
======================
mode:        rule-based
company:     /tmp/agent-demo

— Initialiserer frisk virksomhedsmappe —
  ✓ company init OK (/tmp/agent-demo)

— Spawner MCP-server —
  ✓ MCP klar — 52 tools registered

— Importerer bank-CSV —
  ✓ 7 banktransaktioner importeret

— Læser inbox og ingester bilag —
  ✓ 3 EU-leverandør(er) VIES-validated (offline-seed)
  ✓ aws-hosting-2026-05.txt    → DOC-2026-000001 (980,00 DKK)
  ✓ dsb-rejse-2026-05.txt      → DOC-2026-000002 (450,00 DKK)
  ✓ elgiganten-hardware-…      → DOC-2026-000003 (12.000,00 DKK)
  ✓ google-workspace-2026-05   → DOC-2026-000004 (750,00 DKK)
  ✓ openai-2026-05.txt         → DOC-2026-000005 (425,00 DKK)
  ✓ restaurant-2026-05.txt     → DOC-2026-000006 (1.205,00 DKK)

— Foreslår og bogfører matches —
  ✓ Bogført DSB                 -450,00 DKK → konto 3050 (Rejse og transport)
  ✓ Bogført AWS EMEA           -980,00 DKK → konto 3020 (Hosting, reverse_charge)
  ✓ Bogført Elgiganten A/S   -12.000,00 DKK → konto 3120 (Hardware)
  ✓ Bogført OpenAI Ireland Ltd -425,00 DKK → konto 3010 (AI-værktøjer, reverse_charge)
  ✓ Bogført Google Ireland     -750,00 DKK → konto 3000 (Software og SaaS)

  … 2 banktransaktion(er) sprunget over:
    · bank-tx 7 "Stripe payout"        — ingen høj-confidence match
    · bank-tx 6 "Restaurant Madklubben" — ingen høj-confidence match

— Exceptions-kø —
  ! [medium] #7 UNMATCHED_BANK_TRANSACTION: Bank transaction 7 is still unmatched
  ! [medium] #6 UNMATCHED_BANK_TRANSACTION: Bank transaction 6 is still unmatched

— Momsrapport (2026-05) —
  ✓ udgående moms 351,25 DKK, indgående moms 2.991,25 DKK, netto -2.640,00 DKK

— Audit chain —
  ✓ hash-kæde intakt (5 entries)

— System healthcheck —
  ✓ alle kerne-filer findes

=== Rentemester agent-demo, kørsel afsluttet ===
  • 6 bilag ingested
  • 7 bank-transaktioner importeret
  • 5 udgifter bogført automatisk
  • 2 i exception queue
  • Audit-chain: OK (5 entries)
  • Næste momsangivelse: Q2 2026, udgående moms 351,25 DKK, indgående moms 2.991,25 DKK
  • Tid brugt: 0.3 sekunder
```

## Hvad det her viser

1. **MCP-overfladen er sælgelig.** Hele kørslen sker over `tools/call` —
   ingen direkte DB-skrivninger, ingen genveje. En agent kan tale samme
   sprog som CLI'en.
2. **Reglerne afgør.** Når reverse-charge skal anvendes, eller når et
   bilag mangler VIES-validering, blokerer ledgeren — agenten kan ikke
   omgå reglen. Restaurant-bon'en uden formålsbeskrivelse er det
   klassiske eksempel: agenten ser den, men kan ikke bestemme om det er
   kundebevirtning (delvis fradrag), intern frokost (ingen fradrag),
   eller privat (overhovedet ikke bogføres) — så den lander i exception-
   køen.
3. **Audit-chain er intakt.** Hver bogføring tilføjer en hash-linket
   journal-post. Vi verificerer kæden til sidst. Det er rygraden i
   "ledgeren håndhæver".
4. **Demoen er hermetisk.** Ingen netværk, ingen API-keys. Det vigtigste
   sælgelige løfte: når jeg viser det her i et møde, kører det.

## Hvordan koden hænger sammen

`run.ts` indeholder en lille `McpClient`-klasse (samme stdio-pattern som
`scripts/smoke-mcp.ts`). Den taler udelukkende:

| Steg | MCP-tool | Klassifikation |
|------|----------|----------------|
| 1 | `initialize` + `tools/list` | handshake |
| 2 | `bank_import` | write-reversible |
| 3 | `documents_ingest` (× 6) | write-reversible |
| 4 | `bank_suggest_matches` | read |
| 5 | `expense_book` (× 5) | write-irreversible |
| 6 | `exceptions_list` | read |
| 7 | `vat_report` | read |
| 8 | `audit_verify` | read |
| 9 | `system_healthcheck` | read |

Regelbasen ligger i `SUPPLIER_RULES` øverst i `run.ts`. I et rigtigt
agent-setup ville Claude/anden LLM blive prompted med kontoplanen og
træffe disse valg selv — `--mode claude` er stubbet til at vise hvor
det udvidelsespunkt sidder.

## Bonus: video

Hvis du vil optage en asciinema af kørslen og committe den:

```
asciinema rec assets/agent-demo.cast \
  --command "bun examples/agent-demo/run.ts --company /tmp/agent-demo"
```
