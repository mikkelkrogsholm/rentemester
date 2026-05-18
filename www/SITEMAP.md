# rentemester.dk — Sitemap (arbejdsdokument)

**Positionering:** Rentemester er et open-source CLI-værktøj til dansk bogføring. Site = præsentation + dokumentation + henvisning til GitHub. Ingen hosted app, ingen pricing, ingen signup. Folk kloner repoet og kører det selv.

**Sprog:** kun dansk.

**Tre SEO-lag:**
1. **Projekt / brand** — fortæller hvad Rentemester er og hvorfor det findes
2. **Dokumentation** — installation, brug, CLI-reference, regler
3. **Viden (programmatic SEO)** — evergreen-svar om dansk bogføring, genereret fra `rules/dk/`. Det er det der trækker organisk trafik.

**Beslutninger truffet:**
- App-subdomæne: bortfalder (intet hosted produkt).
- `/blog` foretrukket frem for `/indsigter` — højere søgevolumen, dev-publikum genkender den.
- Kun dansk. Ingen `hreflang`, ingen `/en/`.

---

## 1. Forside & projekt-præsentation

| URL | Intent | Primær keyword | Schema |
|---|---|---|---|
| `/` | "Hvad er Rentemester" + GitHub-CTA | "open source dansk bogføring" | `SoftwareSourceCode`, `Organization` |
| `/hvorfor` | Manifest — hvorfor agent-first OSS-bogføring | "ai bogholderi" | `Article` |
| `/sådan-virker-det` | Arkitektur: agent + ledger + regler | "hvordan fungerer ai bogføring" | `HowTo` |
| `/funktioner` | Feature-oversigt med eksempler | "bogføringsprogram funktioner" | `SoftwareSourceCode` |

### Funktions-deep-dives
| URL | Indhold |
|---|---|
| `/funktioner/agent` | Den AI-agent der bogfører |
| `/funktioner/ledger` | Append-only kassebog |
| `/funktioner/moms` | Momsindberetning |
| `/funktioner/signering` | Kryptografisk signering (ed25519) — link til `/tillid/verificer` |
| `/funktioner/regler` | Hvordan `rules/dk` virker — link til GitHub |
| `/funktioner/dashboard` | CLI-dashboardet |
| `/funktioner/mcp` | MCP-integration for AI-klienter |

---

## 2. Dokumentation — `/docs/`

Markdown-genereret docs. Hver side har "Rediger på GitHub"-link.

### 2.1 Kom i gang
- `/docs` — docs-forside
- `/docs/installation` — `bun install`, krav, første kommando
- `/docs/hurtig-start` — 5-minutters tour
- `/docs/eksempel` — komplet workflow fra bilag til momsindberetning

### 2.2 Brug
- `/docs/cli` — alle CLI-kommandoer, oversigt
- `/docs/cli/[kommando]` — én side pr. kommando (`init`, `add`, `verify`, `dashboard`, `smoke`, …)
- `/docs/agent` — sådan kører du agenten
- `/docs/agent/mcp` — MCP-server setup
- `/docs/agent/prompts` — anbefalede prompts
- `/docs/regler` — hvordan regler virker
- `/docs/regler/skriv-egen` — skriv din egen regel
- `/docs/ledger` — ledger-format (append-only, signing)
- `/docs/ledger/verifikation` — verificér integriteten
- `/docs/ledger/backup` — ed25519-backup-signering
- `/docs/import` — bank, MobilePay, faktura
- `/docs/eksport` — moms-rapport, årsregnskab, CSV

### 2.3 Reference
- `/docs/reference/regler` — auto-genereret oversigt over alle regler i `rules/dk/`
- `/docs/reference/regel/[id]` — én side pr. regel
- `/docs/reference/kontoplan` — standard kontoplan
- `/docs/reference/fejlmeddelelser` — error-codes katalog
- `/docs/reference/konfiguration` — config-filer

### 2.4 Bidrag
- `/docs/bidrag` — sådan bidrager du (link til CONTRIBUTING.md)
- `/docs/bidrag/regler` — bidrag med nye regler
- `/docs/bidrag/oversættelse` — bortfalder (kun dansk)
- `/docs/arkitektur` — high-level arkitektur for kontributorer
- `/docs/changelog` — fra CHANGELOG.md

---

## 3. Viden / Programmatic SEO — `/viden/`

**Dette er trafikmaskinen.** Genereret deterministisk fra `rules/dk/`. Hver side:
- TL;DR i toppen (40-60 ord) — AEO-format, perfekt til AI Overviews
- Uddybning, eksempel, kantsager
- "Sådan håndterer Rentemester det" — link til relevant regel i `docs/reference/regel/[id]` + GitHub-link
- Schema: `Article` + `FAQPage` + `HowTo` hvor relevant

### 3.1 Frister `/viden/frister/`
- `/viden/frister/moms` — momsfrister (måned/kvartal/halvår), opdateret kalender
- `/viden/frister/moms/[periode]` — fx `/q1-2026`
- `/viden/frister/a-skat`
- `/viden/frister/årsregnskab`
- `/viden/frister/selvangivelse`
- `/viden/frister/kalender` — fuld 12-måneders kalender (high-traffic)

### 3.2 Moms `/viden/moms/`
- `/viden/moms` — hub
- `/viden/moms/satser` — 25%, 0%, fritaget
- `/viden/moms/eu-handel` — omvendt betalingspligt
- `/viden/moms/import`
- `/viden/moms/repræsentation` — 25%/75%-reglen
- `/viden/moms/bil`
- `/viden/moms/byggeri`
- `/viden/moms/eksport`
- `/viden/moms/fradrag-blandet-anvendelse`

### 3.3 Sådan bogfører du `/viden/sådan-bogfører-du/` (long-tail guldmine)
- `/mobilepay`
- `/udlæg`
- `/kørselsgodtgørelse`
- `/repræsentation`
- `/firmabil`
- `/leasing`
- `/løn`
- `/feriepenge`
- `/udbytte`
- `/anlægsaktiv`
- `/afskrivning`
- `/lagernedskrivning`
- `/tab-på-debitorer`
- `/gavekort`
- `/forudbetaling`
- `/rente-og-gebyr`
- `/valuta`

### 3.4 Kontoplan `/viden/kontoplan/`
- Oversigt + én side pr. kontonummer (`/kontoplan/1310-vareforbrug`)

### 3.5 Bogføringsloven `/viden/bogføringsloven/`
- Oversigt + én side pr. paragraf (`/§14-digital-opbevaring`)
- `/krav-til-bogføringssystem`
- `/sanktioner`

### 3.6 Selskabsformer `/viden/selskabsform/`
- enkeltmandsvirksomhed, aps, a-s, holdingselskab, interessentskab

### 3.7 Ordbog `/viden/ordbog/`
- A-Å indeks + én side pr. term (debet, kredit, kassebog, balance, periodisering, …)
- Korte definitioner (50-100 ord) — optimalt format til AI-citater

### 3.8 For (segment-viden, ikke salgssider)
| URL | Indhold |
|---|---|
| `/viden/for/enkeltmandsvirksomhed` | Bogføring for EMV — guide |
| `/viden/for/aps` | Bogføring for ApS |
| `/viden/for/freelancer` | – |
| `/viden/for/håndværker` | – |
| `/viden/for/webshop` | – |
| `/viden/for/forening` | – |

### 3.9 Værktøjer `/værktøj/` (statiske, gratis — linkbait)
- `/værktøj/momsberegner`
- `/værktøj/kørselsfradrag-beregner`
- `/værktøj/feriepengeberegner`
- `/værktøj/timefaktura-beregner`
- `/skabelon/faktura` (CSV/PDF download)
- `/skabelon/kørselsregnskab`
- `/skabelon/kontoplan-csv`

---

## 4. Tillid & transparens

Open-source er trust-vinklen. Disse sider gør den konkret.

| URL | Formål |
|---|---|
| `/tillid` | Hub: hvorfor du kan stole på Rentemester |
| `/tillid/open-source` | Licens, repo, hvad er åbent |
| `/tillid/append-only` | Bogføringsloven + append-only forklaret |
| `/tillid/signering` | Ed25519-signering forklaret |
| `/tillid/verificer` | **Interaktiv:** indsæt signatur → "valid/invalid". Kører i browseren. |
| `/tillid/audit-trail` | Hvordan revisor verificerer data |
| `/tillid/datasikkerhed` | Lokalt-først, ingen cloud, GDPR-trivielt |
| `/sikkerhed` | Security policy (link fra `/.well-known/security.txt`) |

---

## 5. Blog `/blog/`

Redaktionel, lav frekvens, høj kvalitet. Build-in-public, tekniske dybdedyk, AI+bogføring-essays.

- `/blog` — index
- `/blog/[slug]`
- `/blog/kategori/[tag]` — `byggeri`, `produktopdateringer`, `ai-bogføring`, `compliance`

---

## 6. Projekt-info

| URL | Formål |
|---|---|
| `/om` | Hvem står bag, kort |
| `/kontakt` | Email, GitHub, CVR |
| `/changelog` | Auto-fra CHANGELOG.md |
| `/roadmap` | Kort + link til GitHub projects/issues |
| `/presse` | Logo-pakke, brand-kit |

---

## 7. AEO / LLM

- `/llms.txt` — markdown-summary af site for LLM-crawlers
- `/llms-full.txt` — fuldt indhold dumpet
- Hver `/viden/`-side: TL;DR-blok øverst — direkte svar i 40-60 ord
- Hver `/docs/`-side: code-eksempel + kort forklaring i top

---

## 8. Juridisk & utility

| URL | Index? |
|---|---|
| `/privatlivspolitik` | ✅ (kort — sitet er statisk uden tracking) |
| `/cookies` | ✅ (helst ingen cookies — så siden er kort) |
| `/.well-known/security.txt` | – |
| `/sitemap.xml` | – |
| `/robots.txt` | – |
| `/rss.xml` | – (blog-feed) |

Ingen `/vilkår`, `/databehandleraftale`, `/login`, `/app/*` — der er ikke noget at have vilkår for.

---

## URL-konventioner

- Dansk, lowercase, bindestreger, æøå tilladt
- Ingen trailing slash undtagen root
- Ingen query-params i kanoniske URL'er
- `lang="da"` på `<html>`. Ingen `hreflang`.
- Canonical altid sat eksplicit
- 0 JS som default; interaktive sider (`/tillid/verificer`, værktøjer) hydrerer punktvist (Astro islands)

---

## Globale elementer

- **Header på alle sider:** logo · `/funktioner` · `/docs` · `/viden` · `/blog` · GitHub-stjerne-knap (link til repo)
- **Footer:** kort om-tekst · sitemap-links · GitHub · MIT-licens · CVR · sikkerhed.txt
- **Hver side:** "Rediger på GitHub" hvis siden er genereret fra docs/rules

---

## Build-rækkefølge

1. **Fase 1 — projektsite minimum** (kan lanceres alene)
   `/`, `/hvorfor`, `/sådan-virker-det`, `/funktioner`, `/docs`, `/docs/installation`, `/docs/hurtig-start`, `/om`, `/kontakt`, `/privatlivspolitik`, `/sitemap.xml`, `/robots.txt`, `/llms.txt`
2. **Fase 2 — tillid + dokumentation**
   `/tillid/*` (inkl. `/tillid/verificer` interaktiv), `/docs/cli/*`, `/docs/reference/*`
3. **Fase 3 — programmatic SEO v1**
   `/viden/frister/moms`, `/viden/sådan-bogfører-du/*` (top 10), `/viden/ordbog/*`, `/værktøj/momsberegner`
4. **Fase 4 — programmatic SEO v2**
   `/viden/bogføringsloven/*`, `/viden/kontoplan/*`, `/viden/moms/*` (fuld), resterende `/viden/sådan-bogfører-du/*`
5. **Fase 5 — segmenter + redaktionelt**
   `/viden/for/*`, `/blog/*`, resterende værktøjer

---

## Åbent spørgsmål

- Skal `/docs/reference/regler` og `/viden/*` genereres som del af main repo's build (så indhold og kode aldrig drifter fra hinanden), eller skal www være eget repo der pull'er reglerne ind ved build? Min anbefaling: monorepo — `www/` her i samme repo, build trækker `rules/dk/` direkte. Det matcher den deterministiske disciplin fra ledger.
