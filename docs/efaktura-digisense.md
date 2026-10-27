# E-faktura via Digisense — opsætning og brug

Sådan kobler du Rentemester på **Digisense**, så en virksomhed kan **sende og
modtage** e-fakturaer (OIOUBL / Peppol BIS 3.0) over **NemHandel** og **PEPPOL**.
Digisense er Rentemesters compliance-partner og leverer det **certificerede
access point** — Rentemester genererer fakturaen, Digisense transporterer den.

> Status: det eksisterende standardflow er testet end-to-end mod DigiSense TEST
> med ægte OIOUBL-validering, acknowledged levering og deduplikeret inbound.
> PDF-vedhæftningen er også testet end-to-end i DigiSense TEST: en syntetisk
> faktura med en registreret testafsenders CVR blev forhåndskontrolleret, udstedt
> og leveret til licensens test-GLN. Den modtagne PDF var byte-identisk med den
> godkendte PDF, og et gentaget afsendelseskald returnerede det eksisterende
> resultat. Produktion
> er ikke aktiveret eller prøvet og kræver en særskilt go-live-godkendelse. Se
> [Test vs. produktion](#test-vs-produktion).

Relateret: [peppol-nemhandel.md](peppol-nemhandel.md) (format/transport-baggrund),
[mcp-tool-surface.md](mcp-tool-surface.md) (agent-/MCP-kontrakten),
[compliance/requirements.md](compliance/requirements.md).

---

## Overblik — fire trin

| Trin | Hvad | Kommando (CLI) | MCP-tool |
|------|------|----------------|----------|
| 0 | Få en API license-key hos Digisense | *(uden for systemet — se nedenfor)* | — |
| 1 | Gem nøglen i Rentemester | `efaktura konfigurer` | `efaktura_konfigurer` |
| 2 | Registrér virksomheden i NemHandel | `efaktura registrer` | `efaktura_registrer` |
| 2b | Sikker onboarding fra ledgerprofilen | `efaktura onboard` | `efaktura_onboard` |
| 2c | Lokal readiness (redacted) | `efaktura onboarding-status` | `efaktura_onboarding_status` |
| 2a | Registrér test-GLN (kun testmiljø) | `efaktura registrer-test-gln --company <path> --confirm yes` | — |
| 3 | Send en udstedt e-faktura | `invoice transmit-digisense` | `efaktura_send` |
| 4 | Modtag indkomne e-fakturaer (poll) | `efaktura modtag` | `efaktura_modtag` |
| 4b | Poll alle aktive workspace-virksomheder | `efaktura modtag-workspace` | `efaktura_modtag_workspace` |

Datamodel hos Digisense: **license (din nøgle) → company (companyKey pr. CVR) →
participant (inbound + outbound)**. Én license-key dækker **flere virksomheder**;
hver virksomhed får sin egen `companyKey` ved registrering (trin 2), og
Rentemester husker koblingen `companyKey ↔ virksomhed` for dig.

---

## Trin 0 — Få adgang hos Digisense

Du skal bruge en **API license-key** fra Digisense. Den er et **secret** og
dækker hele din licens (alle dine virksomheder/CVR-numre).

**Sådan får du den:**

> ⚠️ **TODO — bekræftes med Digisense.** Den officielle, delbare proces for at
> anmode om en license-key (kontaktkanal, evt. pris, vilkår) er endnu ikke
> dokumenteret her. Udfyld dette afsnit med Digisense' egen onboarding-tekst, så
> en ny bruger (fx en kollega eller et søsterselskab) selv kan komme i gang.

Det vi ved i dag:

- API'et og dets dokumentation ligger på **<https://api.digisense.dk>**
  (Scalar-UI; OpenAPI-spec på `…/ap/api/rest/openapi-spec.json`).
- Leverandør: **Digisense A/S** (CVR 32082378, Risskov).
- Nøglen udleveres typisk som et delt secret. **Opbevar den sikkert** og indtast
  den aldrig i en faktura, et bilag eller andet, der havner i bogføringen.
- Vil du have en nøgle, der er **låst til én virksomhed**, har API'et et
  `issue-api-key-for-company`-endpoint — bed Digisense om det, eller udsted den
  selv senere mod licens-nøglen.

Når du har nøglen, fortsæt til trin 1.

---

## Trin 1 — Konfigurér nøglen i Rentemester

Gemmer license-key'en i secret-laget. Den lander i
`<virksomhedsmappe>/config/digisense.json` (rettigheder `0600`, **gitignored**),
og rammer **aldrig** ledger'en.

```bash
rentemester efaktura konfigurer \
  --company <sti-eller-slug> \
  --api-license-key <din-license-key> \
  --actor user:<dig> --confirm yes \
  --environment test
```

- `--environment test` (standard) peger på sandbox `test-api.digisense.dk`;
  `production` peger på `api.digisense.dk`. **Start med `test`.**
- Dette er en **forudsætning** for trin 2–4: uden en gemt nøgle fejler de med
  *"Digisense er ikke konfigureret"*.
- Nøglen valideres reelt først ved det første rigtige kald (trin 2). En forkert
  nøgle giver en tydelig fejl-envelope dér.

---

## Trin 2 — Registrér virksomheden i NemHandel

Registrerer CVR'et hos Digisense: opretter `company` (får `companyKey`) og
registrerer virksomheden som **både** `outbound` (kan sende) **og** `inbound`
(kan modtage). `webhookUrl` sættes altid til `null` — Rentemester **poller** selv
(ingen always-on server nødvendig).

```bash
rentemester efaktura registrer \
  --company <sti-eller-slug> \
  --cvr DK12345678 \
  --company-name "Min Virksomhed ApS" \
  --confirm yes
```

- `--network nemhandel` er standard; brug `--network peppol` for PEPPOL.
- **Idempotent:** kører du den igen med samme CVR, duplikeres intet.
- Skrivende handling: kræver `--confirm yes` og en actor (logges i `audit_log`).
- Resultatet indeholder bl.a. `companyKey` og hvilke retninger der blev
  registreret. Rentemester husker koblingen, så du sjældent skal angive
  `companyKey` manuelt senere.

---

## Trin 3 — Send en e-faktura

### Godkendt ekstern faktura-PDF

En ekstern PDF kan bruges som fakturaens uforanderlige PDF og indlejres i
OIOUBL-fakturaen. Modtageren skal have aftalt indlejrede bilag. Først vises det
næste mulige nummer uden at reservere det:

```bash
rentemester invoice preview-number --company <sti-eller-slug> --issue-date 2026-05-16
```

Nummeret kan ændre sig, hvis en anden faktura udstedes imens. Lav PDF'en med
nummeret og de endelige fakturaoplysninger, og kør derefter:

```bash
rentemester invoice preflight-approved-pdf --company <sti-eller-slug> \
  --input faktura.json --approved-pdf faktura.pdf \
  --attachment-agreed yes --actor user:<dig> --confirm yes
```

Kontrollen læser PDF'ens tekstlag, sammenholder centrale oplysninger, validerer
den præcise XML hos DigiSense og kontrollerer, at EAN-modtageren er registreret
til samme dokumenttype. Den prøver OIOUBL 2.1 først; ved et 404-opslag kan den
prøve OIOUBL 2.02 med ny XML-validering og et særskilt opslag. Andre fejl
omgås ikke. Ældre godkendte XML-beviser genskabes i deres oprindelige version.
OIOUBL 2.1 bruger UBL 2.1 og `CustomizationID` `OIOUBL-2.1`, jf.
[Økonomistyrelsens beskrivelse](https://oes.dk/media/xq2b3iwe/nyt-i-navision-stat-112003.pdf).
Kontrollen reserverer intet nummer. Svaret indeholder et
`preflightId`, PDF- og XML-hashes samt fakturagrundlaget. **Gennemgå PDF'en
visuelt mod hele grundlaget**: tekstforekomst alene kan ikke bevise, at et
beløb eller en reference står i den rigtige sammenhæng. Et vellykket opslag
er heller ikke en leveringskvittering eller en garanti for, hvordan modtageren
viser bilaget. Helt momsfri fakturaer med eksplicit `exempt` på alle linjer
understøttes; blandede linjeklassifikationer og PDF'er uden læsbart tekstlag
understøttes ikke i dette forløb.

Hvis en lang linjebeskrivelse næsten matcher, men enkelte ord ikke findes i
PDF'ens tekstlag, afvises kontrollen med de manglende ord. Sammenhold hele
beskrivelsen med PDF'en, også oplysninger placeret i andre felter. Kun efter
denne gennemgang kan kontrollen gentages med `--description-reviewed yes`
(MCP: `descriptionDifferencesReviewed: true`). Den separate bekræftelse af
modtagerens accept af bilaget er fortsat obligatorisk. Det afsluttende
`--pdf-reviewed yes` kræver stadig en visuel kontrol af hele fakturaen.

I DigiSense TEST kan licensens egen test-GLN bekræftes via licensens registrerede
testdeltager, hvis det offentlige opslag returnerer 404. Dette gælder kun for
test-GLN'en i testmiljøet og kræver en indgående NemHandel-registrering med
den passende fakturaprofil. Produktion bruger fortsat det offentlige opslag.

Efter gennemgangen udstedes med samme JSON, PDF og `preflightId`:

```bash
rentemester invoice issue --company <sti-eller-slug> --input faktura.json \
  --approved-pdf faktura.pdf --preflight-id <id> --pdf-reviewed yes \
  --actor user:<dig> --confirm yes
```

Forhåndskontrollen gælder i én time. Ændres nummer, selskabsidentitet,
DigiSense-miljø eller -selskabsnøgle, betalingsoplysninger, fakturagrundlag eller PDF, afvises udstedelsen og
kontrollen skal gentages. Ved udstedelse gemmes de gennemgåede PDF-bytes som
fakturaens kanoniske PDF. Ved eksport og afsendelse verificeres filens hash og
XML'ens identitet igen, og afsendelse kræver samme DigiSense-miljø og
selskabsnøgle; manglende eller ændret evidens stopper afsendelsen.

MCP-operationerne `sales_invoice_number_preview`,
`sales_invoice_approved_pdf_preflight` og `sales_invoice_issue` understøtter
samme forløb. MCP kræver, at PDF'en ligger under virksomhedens
`invoices/drafts`-mappe; brug `attachmentAgreed: true`, derefter
`approvedPdfPath`, `preflightId`, `pdfReviewed: true` og `confirm: true`.
Ingen af disse trin bogfører eller sender fakturaen. Afsendelse sker efter de
normale regler nedenfor.

Sender en allerede **udstedt** offentlig e-faktura gennem Digisense. Flowet er:
`validate-document` (schematron) → `deliver-document` → poll til *delivered*. En
succes bogføres som en **acknowledged PEPPOL-submission**.

```bash
# Udsted først fakturaen som normalt (skal være en offentlig modtager / EAN):
rentemester invoice issue --company <…> --input faktura.json

# Send den via Digisense:
rentemester invoice transmit-digisense \
  --company <sti-eller-slug> \
  --invoice-number 2026-0001 --actor user:<dig> --confirm yes
```

- Brug enten `--invoice-number <no>` eller `--document-id <n>`.
- **Intet `--access-point`:** for Digisense *er* access point'et Digisense selv;
  identiteten udledes deterministisk af `companyKey`.
- **Dobbelt-afsendelse er forhindret:** gentaget transmit af samme faktura er
  idempotent. Hvis en levering bliver sat i kø men ikke når *delivered* indenfor
  poll-budgettet, gemmes en **pending** submission med Digisense' `documentId` —
  et nyt forsøg afvises og beder dig poll'e leverings-status i stedet for at
  levere igen.

---

## Trin 4 — Modtag e-fakturaer

Poller Digisense for nye indkomne fakturaer (`list-received-documents`), følger
pagination, henter hvert nyt dokument og ingester det i bogføringen. **Ingen
webhook / ingen always-on server** — kør kommandoen ved opstart eller på et
interval.

```bash
rentemester efaktura modtag \
  --company <sti-eller-slug> \
  --confirm yes
```

- **Dedup på Digisense' stabile `internalId`:** kører du den igen, ingesteres
  intet dobbelt.
- Et dokument der ikke kan ingestes (validerings-/dublet-fejl) sættes i
  **karantæne**, så det ikke hentes og fejler i en uendelig løkke. Partiel
  succes: én dårlig faktura vælter ikke de øvrige.
- Nyttige flag: `--limit <n>` (side-størrelse, ≤100), `--max-timestamp
  <ISO8601>` (fx `2026-06-01T00:00:00Z`), `--metadata <file.json>` (booking-felter
  der flettes oven på de UBL-afledte), `--force` (tillad logisk dublet),
  `--digisense-company-key <key>` (hvis du har flere registrerede virksomheder).
- Skrivende handling: kræver `--confirm yes` og en actor.

---

## Test vs. produktion

| | Test (sandbox) | Produktion |
|---|---|---|
| `--environment` | `test` | `production` |
| Base-URL | `test-api.digisense.dk` | `api.digisense.dk` |

Kør **hele** forløbet (konfigurer → registrer → send → modtag) i `test` først.
Skift til produktion ved at køre `efaktura konfigurer … --environment production`
igen med din produktions-nøgle.

> ⚠️ **TODO — produktions-go-live.** Bekræft med Digisense, om test- og
> produktions-license-key er den samme eller to forskellige nøgler, og hvad der
> kræves for at gå live (fx rigtig NemHandel-registrering af CVR'et i
> produktion). Indtil dette er afklaret: hold dig til `test`.

---

## For agenter (MCP)

Hele overfladen findes også som MCP-tools, så en agent kan drive forløbet:
`efaktura_konfigurer`, `efaktura_registrer`, `efaktura_send`, `efaktura_status`, `efaktura_modtag`.

Hvis en afsendelse ender som `prepared` med et Digisense documentId, brug
`efaktura leveringsstatus --document-id <lokalt-id> --confirm yes` (eller den kompatible alias
`efaktura status`, eller
`efaktura_status`). Den kalder kun `document-status`, gemmer append-only
statusevidens og kalder aldrig `document-delivery` igen. Når status er
`delivered`, bliver resultatet effektivt `acknowledged` for senere send.

KSeF-værdierne følger Digisense-kontrakten: `PROD`, `TEST` og `DEMO`.
De følger samme forudsætninger og confirm/actor-gates som CLI'en. Se de
autoritative input/output-shapes i [mcp-tool-surface.md](mcp-tool-surface.md).

En typisk agent-sætning: *"Registrér virksomheden CVR DK12345678 i NemHandel"* →
agenten kalder `efaktura_registrer`. *"Hent nye fakturaer"* → `efaktura_modtag`.

## Legal-company boundary

Every ledger is one legal company. DigiSense registration identity is derived from the local company profile (CVR and legal name); conflicting caller values and company keys belonging to another CVR fail before a network request. `efaktura_onboarding_status` is local-only and never exposes the API key or DigiSense `signatureSecret`. `efaktura_onboard` validates auth and idempotently ensures both inbound and outbound registration.

---

## Forudsætninger & sikkerhed (kort)

- **Rækkefølge:** konfigurer (1) før registrer (2); registrer før send/modtag (3–4).
- **License-key er et secret:** den bor kun i `config/digisense.json` og bliver
  aldrig returneret af et tool eller skrevet til ledgeren/audit-loggen.
- **Alt skrivende kræver `--confirm yes` + actor** og logges i `audit_log`.
- **Fejl-envelope:** mangler nøglen, får du *"Digisense er ikke konfigureret"* med
  en henvisning til at køre `efaktura konfigurer` først.

---

## Onboarding af andre brugere

En anden virksomhed der vil bruge Rentemester + Digisense skal altid have sin
egen Rentemester-virksomhedsmappe og sin egen DigiSense-`companyKey`:

1. Opret virksomheden med korrekt juridisk navn og CVR.
2. Kør `efaktura konfigurer` i netop dens mappe. Den samme operatør-license-key
   kan genbruges, hvis DigiSense-aftalen tillader flere CVR-numre; en separat
   kunde/licens skal bruge sin egen nøgle.
3. Kør `efaktura onboard`; Rentemester afleder identiteten fra ledgerprofilen
   og gemmer kun den returnerede `companyKey` i denne virksomheds ledger.
4. Kontrollér `efaktura onboarding-status` før send/modtag.

En `companyKey` må aldrig kopieres mellem virksomhedsmapper. Workspace-polling
bruger hver virksomheds lokale binding og laver actor-/backup-preflight på alle
aktive virksomheder før første netværkskald.

> ⚠️ **TODO — partner-/PR-omtale.** Digisense ønsker at Rentemester nævner dem som
> compliance-partner (og vil selv bruge Rentemester i deres PR). Når den fælles
> ordlyd er aftalt, tilføj den her og evt. i `README`.

---

## Hvad der mangler (samlet TODO-liste)

- [ ] **Trin 0:** Digisense' officielle, delbare proces for at få en license-key.
- [ ] **Go-live:** test- vs. produktions-nøgle, og krav til produktions-registrering.
- [ ] **Partner-/PR-tekst** til omtale af Digisense som compliance-partner.
- [x] **Rigtig sandbox-e2e:** validering, acknowledged outbound, status-idempotens
      og deduplikeret inbound er verificeret mod `test-api.digisense.dk`.
