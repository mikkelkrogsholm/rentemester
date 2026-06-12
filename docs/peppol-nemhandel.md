# E-faktura til det offentlige via NemHandel (Peppol BIS)

Denne vejledning beskriver, hvordan Rentemester sender EAN-fakturaer til
danske offentlige myndigheder, hvad der allerede er bygget, og — vigtigst —
**hvad der konkret skal gøres for at sende en rigtig faktura afsted.**

Den er skrevet til den udvikler eller virksomhedsejer, der skal tage det
sidste skridt: at koble Rentemester på det rigtige NemHandel-net.

Arbejdet spores i [epic #327](https://github.com/mikkelkrogsholm/rentemester/issues/327).

## Indhold

1. [Status — hvad virker, hvad mangler](#1-status)
2. [Sådan hænger e-faktura-nettet sammen](#2-arkitektur)
3. [Hvad Rentemester kan i dag](#3-hvad-rentemester-kan-i-dag)
4. [Hvad der mangler for at sende rigtigt](#4-hvad-der-mangler)
5. [Trin for trin: kom i gang med rigtig afsendelse](#5-kom-i-gang)
6. [Implementér den ægte PeppolTransmitter](#6-peppoltransmitter)
7. [Kobl det til CLI'en](#7-cli)
8. [Test før produktion](#8-test)
9. [Er dokumentet gyldigt? (validering)](#9-validering)
10. [Kode- og datamodelreferencer](#10-kodereferencer)
11. [Kendte begrænsninger og TODO](#11-begraensninger)
12. [Referencer og kilder](#12-referencer)

---

## 1. Status

Rentemester bygger sin del af e-fakturering uden afhængighed af en
**kommerciel** tredjeparts-service. Det offentlige (Erhvervsstyrelsen)
udgiver hele værktøjskassen gratis, så Rentemester kan optræde som sit eget
access point.

| Fase | Indhold | Status |
|------|---------|--------|
| **Fase 1** | Korrekt e-faktura-format (Peppol BIS Billing 3.0) | ✅ Færdig og testet |
| **Fase 2** | Transport-sømmen (`transmitPublicEInvoicePeppol`) | ✅ Færdig og testet |
| **Fase 3** | Den ægte transport via Oxalis-access point + CLI | ⛔ Mangler — kræver indkøb |

Fase 3 er **ikke** blokeret af kode. Den er blokeret af to ting, kun
virksomhedsejeren kan skaffe:

- et **MitID systemcertifikat (FOCES3)**, og
- en **endpoint-registrering i NemHandelsRegistret**.

Når de to ting findes, er den resterende kode lille og veldefineret — den er
beskrevet i afsnit 6 og 7.

---

## 2. Arkitektur

### 2.1 4-hjørnemodellen

Det danske offentlige e-faktura-net er **NemHandel**, som er ved at flytte
over på **eDelivery/Peppol**-infrastrukturen. Man kan ikke aflevere en faktura
direkte i en myndigheds indbakke — der er altid et access point imellem:

```
Afsender            Afsenders AP          Modtagers AP        Myndighed
(Rentemester)  -->  (Oxalis sidecar)  -->  (myndighedens AP)  -->  (indbakke)
                          |
                          v
                   NemHandelsRegistret (NHR)
                   slår modtagerens EAN op
                   og finder dens AS4-endpoint
```

- **NemHandelsRegistret (NHR)** — gratis offentligt opslagsregister (en
  eDelivery SMP). Her slås en modtagers EAN-nummer op og oversættes til dens
  AS4-endpoint. Alle offentlige myndigheder er registreret.
- **Access point** — den server, der signerer og transporterer AS4-beskeder.
  Erhvervsstyrelsen udgiver en gratis open source-referenceimplementering
  (Oxalis), så man kan hoste sit eget. ISO 27001 / OpenPeppol-medlemskab
  gælder **kun** for kommercielle udbydere på det grænseoverskridende
  Peppol-net — ikke for at sende sine egne danske B2G-fakturaer.
- **MitID systemcertifikat (FOCES3)** — organisationens digitale signatur.
  Den lægges i access point'ets keystore og bruges til at signere og sikre
  AS4-beskederne. Den kommer aldrig ind i Rentemesters bogføringstilstand.

### 2.2 Dokument vs. transport

Rentemester deler bevidst opgaven i to:

1. **Dokumentet** — den deterministiske kerne genererer en gyldig
   Peppol BIS Billing 3.0-faktura (XML). Det er fuldt implementeret og testet.
2. **Transporten** — at aflevere XML'en gennem et access point. Det er en
   side-effekt uden for ledgeren, modelleret som en **udskiftelig adapter**
   (`PeppolTransmitter`). Selve AS4-kaldet udføres af Oxalis.

Det matcher projektets princip: *agenten handler, reglerne afgør, ledgeren
håndhæver* — dokumentgenerering er deterministisk og auditbar; transport er
en isoleret, injiceret afhængighed.

---

## 3. Hvad Rentemester kan i dag

Al kode ligger i [`src/core/public-einvoice.ts`](../src/core/public-einvoice.ts).

### 3.1 Funktioner

| Funktion | Hvad den gør | CLI-kommando |
|----------|--------------|--------------|
| `exportPublicEInvoicePreview` | Rentemester-internt preview-XML (ikke til transport) | `invoice export-public` |
| `exportPublicEInvoiceOioUbl` | Den rigtige **Peppol BIS Billing 3.0**-faktura | `invoice export-public-oioubl` |
| `submitPublicEInvoicePeppol` | Bygger en submission-kuvert og registrerer forsøget | `invoice submit-public-peppol` |
| `transmitPublicEInvoicePeppol` | **Transport-sømmen** — kalder en injiceret transmitter og registrerer udfaldet | *(ingen CLI endnu)* |

### 3.2 Formatet

`exportPublicEInvoiceOioUbl` genererer en Peppol BIS Billing 3.0-faktura
(UBL 2.1). Det er bevidst valgt frem for det aflyste OIOUBL 3.0 og det
under-udfasning klassiske OIOUBL 2.1: Peppol BIS Billing 3.0 accepteres af
**alle** danske offentlige myndigheder og er det format, NemHandel selv
migrerer over på.

Nøgleidentifikatorer i den genererede XML:

```xml
<cbc:CustomizationID>urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0</cbc:CustomizationID>
<cbc:ProfileID>urn:fdc:peppol.eu:2017:poacc:billing:01:1.0</cbc:ProfileID>
```

Deltager-identifikatorer (ISO 6523-skemaer):

- **Køber** (den offentlige myndighed): `<cbc:EndpointID schemeID="0088">` —
  skema 0088 er GLN/EAN.
- **Sælger** (virksomheden): `<cbc:EndpointID schemeID="0184">` — skema 0184
  er dansk CVR (DIGSTORG).

Obligatoriske felter, som dokumentet indeholder (EN16931): begge parters
`EndpointID`, landekode på begge postadresser, `PartyLegalEntity` med
`RegistrationName`, moms-totaler og linje-detaljer.

> **Navngivning:** funktions- og CLI-navne indeholder stadig "OioUbl" af
> hensyn til interface-stabilitet, selvom outputtet nu er Peppol BIS. Det er
> et bevidst valg — se afsnit 11.

### 3.3 Determinisme, audit og idempotens

- Al XML-generering er **deterministisk** — samme input giver byte-for-byte
  samme output (ingen tidsstempler, ingen tilfældige id'er).
- Hvert eksport- og submission-skridt skriver et **audit-event**.
- `peppol_submissions`-tabellen er **append-only** (triggere afviser UPDATE og
  DELETE). En submission registreres én gang; en transmission registreres som
  en ny `acknowledged`-række.
- `submitPublicEInvoicePeppol` og `transmitPublicEInvoicePeppol` er
  **idempotente** på en afledt nøgle (faktura + access point + modtager), så
  et gentaget kald ikke dublerer arbejdet.

### 3.4 Transport-sømmen

`transmitPublicEInvoicePeppol(db, input, transmitter)` er klar til at modtage
en rigtig transmitter:

- Den genererer OIOUBL'en, validerer access-point-konfigurationen og
  kalder den injicerede `transmitter`.
- **Succes** → fakturaen registreres som en `acknowledged` submission med
  transmissions-id, og et `public_einvoice_peppol_transmission`-audit-event
  skrives.
- **Fejl** → kun et audit-event skrives (ingen submission-række), så et
  senere retry stadig kan nå `acknowledged`.
- Allerede transmitteret → transmitteren kaldes ikke igen.

Det eneste, der mangler, er den konkrete `transmitter` — se afsnit 6.

---

## 4. Hvad der mangler

Tjekliste for at gå fra "genererer korrekt XML" til "sender en rigtig faktura":

- [ ] **MitID systemcertifikat (FOCES3)** anskaffet — afsnit 5.1
- [ ] **Endpoint registreret i NemHandelsRegistret** — afsnit 5.2
- [ ] **Oxalis-access point deployet** (sidecar) — afsnit 5.3
- [ ] **Den ægte `PeppolTransmitter` implementeret** — afsnit 6
- [ ] **CLI-kommando `invoice transmit-public-peppol`** koblet på — afsnit 7
- [ ] **Testet mod NemHandels DEMO-miljø** — afsnit 8

---

## 5. Kom i gang

### 5.1 Skaf et MitID systemcertifikat (FOCES3)

Access point'et signerer AS4-beskeder med et **MitID systemcertifikat**
(tidligere kaldet et organisations-/FOCES-certifikat). Det udstedes til
virksomhedens CVR-nummer.

- Bestilles via **MitID Erhverv**. En medarbejder med de rette rettigheder
  kan acceptere vilkårene på virksomhedens vegne.
- Certifikatet leveres som en nøglefil (typisk `.p12`/PKCS#12) med en
  adgangskode. Den lægges i Oxalis' keystore (afsnit 5.3) — **aldrig** i
  Rentemesters database.
- Certifikater udløber (ca. årligt). Planlæg fornyelse; NHR har en
  "opdater certifikat"-funktion til at skifte uden nedetid.
- Til **test** findes et MitID TEST-certifikat, der virker mod DEMO-miljøet
  (afsnit 8) — start altid der.

### 5.2 Registrér et endpoint i NemHandelsRegistret (NHR)

Alle eDelivery-endpoints skal være registreret i NHR. For en **afsender** er
det tilstrækkeligt med en *teknisk* registrering — selve afsendelsen udgør det
kommercielle tilsagn, så en afsender behøver ikke den fulde modtager-onboarding.

Der er to veje:

- **NHR Web** — et selvbetjeningssite, hvor man logger ind med MitID Erhverv
  og opretter/redigerer registreringer manuelt. Velegnet til få registreringer.
- **NHR PoRS** — et programmatisk REST/XML-API (OpenAPI-dokumenteret), så
  registreringer kan vedligeholdes fra et andet system.

En registrering kræver:

- virksomhedens deltager-id — for en dansk virksomhed CVR under skema `0184`
  (formateret `0184:DK########`), eller et GLN under `0088`,
- en reference til MitID systemcertifikatet,
- en eller flere understøttede profiler (Nemhandel eDelivery AS4).

> Den nøjagtige fremgangsmåde er beskrevet i Erhvervsstyrelsens vejledning
> *"Registrering af Nemhandel eDelivery endepunkter"* — se afsnit 12.

### 5.3 Deploy et Oxalis-access point (sidecar)

Erhvervsstyrelsens referenceimplementering hedder **Oxalis NG** og er open
source (Apache 2.0):

- Repo: `https://git.erst.dk/openebusiness/nemhandeledelivery/oxalis`
- Version 2.0 (udgivet april 2025) bygger på Oxalis NG.
- Krav: **Java 11** og **Tomcat 10.x**. Bygges med Maven.
- Moduler: `oxalis-ng-standalone` (CLI-afsender) og `oxalis-ng-war`
  (webapp til Tomcat, eksponerer et REST-API).

Kort opskrift (følg repoets *Operations Guide* for detaljer):

1. Installér Java 11 + Tomcat 10.
2. Byg: `mvnw clean install` → giver en `oxalis.war`.
3. Deploy `oxalis.war` til `<CATALINA_BASE>/webapps`.
4. Opret `<CATALINA_BASE>/.oxalis/oxalis.conf` med tre sektioner:
   - **keystore** — sti, alias og adgangskode til MitID systemcertifikatet,
   - **jdbc** — databaseforbindelse (Oxalis understøtter MySQL),
   - **lookup/reader** — provider-konfiguration til NemHandels SML/SMP.
5. Kør mod **DEMO-miljøet** først (afsnit 8).

Anbefalet drift: Oxalis kører som en **sidecar-container** ved siden af
Rentemesters company-container. Rentemester taler kun med Oxalis lokalt;
certifikatet bor i Oxalis.

---

## 6. PeppolTransmitter

Det er den ene funktion, der mangler. Kontrakten er allerede defineret i
[`src/core/public-einvoice.ts`](../src/core/public-einvoice.ts):

```ts
export type PeppolTransmissionOutcome =
  | { ok: true; transmissionId: string; transmittedAt: string }
  | { ok: false; error: string };

export type PeppolTransmitter = (input: {
  oioublXml: string;
  oioublSha256: string;
  receiverEndpointId: string;        // fx "0088:5790000000001"
  accessPoint: PeppolAccessPointConfig;
}) => Promise<PeppolTransmissionOutcome> | PeppolTransmissionOutcome;
```

En transmitter skal:

1. Tage `oioublXml` og aflevere den til Oxalis.
2. Lade Oxalis slå `receiverEndpointId` op i NHR, signere med MitID-certifikatet
   og udføre AS4-transmissionen.
3. Returnere `{ ok: true, transmissionId, transmittedAt }` ved succes
   (transmissionId = AS4-kvitteringens/beskedens id), ellers
   `{ ok: false, error }`.

`transmitPublicEInvoicePeppol` klarer resten: idempotens, registrering i
`peppol_submissions`, audit-spor og fejlhåndtering.

### 6.1 To integrationsstile

- **WAR + REST** (anbefalet til sidecar-drift): Oxalis `oxalis-ng-war` kører i
  Tomcat og eksponerer et REST-API. Transmitteren bliver en HTTP-klient, der
  POST'er OIOUBL'en til den lokale Oxalis.
- **CLI-subproces**: `oxalis-ng-standalone` kaldes som en underproces med
  OIOUBL-filen som argument.

> ⚠️ **Det ene, der skal verificeres:** den nøjagtige form på Oxalis NG's
> REST-endpoint (URL, request-/response-skema) hhv. `oxalis-ng-standalone`s
> argumenter er ikke bekræftet i denne kodebase. Bekræft det mod
> referenceimplementeringens *Operations Guide* og *system documentation*,
> før transmitteren tages i brug. Skitsen nedenfor viser strukturen — ikke
> en verificeret kontrakt.

### 6.2 Skitse (WAR + REST)

Læg den i en ny fil, fx `src/core/peppol-oxalis-transmitter.ts`. Den hører
til transport-laget, ikke kernen, og kender intet til ledgeren:

```ts
import type { PeppolTransmitter } from "./public-einvoice";

/**
 * Bygger en PeppolTransmitter, der afleverer OIOUBL'en til et lokalt,
 * selv-hostet Oxalis-access point via dets REST-API.
 *
 * VERIFICÉR endpoint-stien og request-/response-skemaet mod Oxalis NG's
 * Operations Guide, før den bruges i produktion.
 */
export function createOxalisTransmitter(oxalisBaseUrl: string): PeppolTransmitter {
  return async ({ oioublXml, receiverEndpointId, accessPoint }) => {
    try {
      const response = await fetch(`${oxalisBaseUrl}/api/send`, {
        method: "POST",
        headers: { "Content-Type": "application/xml" },
        body: oioublXml,
        // Modtager + afsender medsendes som Oxalis kræver det — bekræft formen.
        // receiverEndpointId fx "0088:5790000000001"
        // accessPoint.senderEndpointId fx "0184:DK12345678"
      });
      if (!response.ok) {
        return { ok: false, error: `oxalis svarede ${response.status}` };
      }
      const result = await response.json();
      return {
        ok: true,
        transmissionId: result.transmissionId,   // bekræft feltnavn
        transmittedAt: result.timestamp,          // ISO8601
      };
    } catch (error) {
      return { ok: false, error: error instanceof Error ? error.message : String(error) };
    }
  };
}
```

Trust-grænse: transmitteren taler kun med den lokale Oxalis. Certifikat og
nøgler bor i Oxalis' keystore — de kommer aldrig ind i Rentemesters
SQLite-fil eller `peppol_submissions`.

---

## 7. CLI

Når transmitteren findes, kobles den på en CLI-kommando, der spejler den
eksisterende `invoice submit-public-peppol`:

```
invoice transmit-public-peppol --company <path> \
  (--document-id <n> | --invoice-number <no>) \
  --access-point <file.json>
```

Handler-skitse (i `src/cli/invoice.ts`):

```ts
// 1. Læs access-point-konfigurationen fra --access-point-filen.
//    { accessPointId, endpointUrl, senderEndpointId } — credentials er IKKE
//    i denne fil; de bor i Oxalis' keystore.
// 2. Byg transmitteren: createOxalisTransmitter(oxalisBaseUrl).
// 3. Find invoiceDocumentId ud fra --document-id / --invoice-number.
// 4. Kald: await transmitPublicEInvoicePeppol(db, { invoiceDocumentId,
//    accessPoint }, transmitter).
// 5. Skriv resultatet som JSON til stdout (ok, status, transmissionId, errors).
```

Husk også at registrere kommandoen i `src/cli-meta.ts` (så `--help` og
CLI-kontrakten er dækkende) og — hvis agenter skal kunne sende — som et
MCP-tool i `src/mcp/`.

---

## 8. Test

**Test aldrig direkte mod produktion først.** NemHandel har et DEMO-miljø:

- Et **eDelivery DEMO-endepunkt** (fx `edel-demo.nemhandel.dk`), der bruger et
  **MitID TEST-certifikat**.
- Registrér et test-endpoint i NHR's testmiljø, send en faktura til et
  demo-modtager-endpoint, og bekræft, at AS4-kvitteringen kommer retur.

Lag for lag:

- **Orkestreringen** (`transmitPublicEInvoicePeppol`) er allerede dækket af
  unit-tests med fake-transmittere — se
  [`tests/unit/public-einvoice.test.ts`](../tests/unit/public-einvoice.test.ts),
  describe-blokken *"public e-invoice PEPPOL transmission"*.
- **Transmitteren** bør have sine egne tests mod en stub-Oxalis (en lille
  fake HTTP-server), så argument-konstruktion og svar-parsing er dækket.
- **Ende-til-ende** verificeres mod DEMO-miljøet — det kan ikke unit-testes.

Gates som altid: `bun test` + `bun run smoke`.

---

## 9. Validering

### 9.1 Hvad dokumentet indeholder

`exportPublicEInvoiceOioUbl` afviser eksport, hvis obligatoriske felter
mangler (`validateOioUblPayload`): udsteder-/forfaldsdato, sælgers og købers
navn/adresse/identifikatorer, EAN på 13 cifre, valuta, totaler og linjer.
Faktura-aritmetikken (linjesummer, moms, brutto) håndhæves allerede ved
udstedelse af `validateInvoice`, så et udstedt dokument er altid konsistent.

Den genererede XML indeholder alle EN16931-obligatoriske elementer for en
simpel standard-momset faktura.

### 9.2 Schematron-validering

Det fulde bevis for, at en faktura er gyldig Peppol BIS, er validering mod den
officielle **schematron**. Referenceimplementeringen leverer artefakterne
(v2.0 indeholder bl.a. OIOUBL 2.1 schematron v1.15.2 samt Peppol DK CIUS og
Peppol BIS3).

Anbefalet næste skridt: hent schematron-artefakterne fra referenceimpl.-repoet
og kør den genererede OIOUBL igennem som et eksplicit valideringstrin (og
gerne i CI). Det kræver en XSLT-processor og er bevidst ikke gjort endnu — se
afsnit 11.

---

## 10. Kodereferencer

| Hvad | Hvor |
|------|------|
| Al e-faktura-logik | [`src/core/public-einvoice.ts`](../src/core/public-einvoice.ts) |
| OIOUBL/Peppol BIS-bygger | `buildPublicEInvoiceOioUblXml` |
| Felt-validering før eksport | `validateOioUblPayload` |
| Transport-sømmen | `transmitPublicEInvoicePeppol`, `PeppolTransmitter` |
| EAN-normalisering | [`src/core/ean.ts`](../src/core/ean.ts) |
| `peppol_submissions`-tabellen | [`src/core/schema.sql`](../src/core/schema.sql) |
| Køber-metadata (`eanNumber`, `publicRecipient`) | `InvoiceBuyer` i [`src/core/invoice.ts`](../src/core/invoice.ts) |
| Regler | `DK-INVOICE-PUBLIC-*` / `DK-PEPPOL-SUBMIT-001` i `rules/dk/invoices.yaml` |
| Tests | [`tests/unit/public-einvoice.test.ts`](../tests/unit/public-einvoice.test.ts), [`tests/unit/public-einvoice-cli.test.ts`](../tests/unit/public-einvoice-cli.test.ts) |

En faktura markeres som offentlig ved at sætte `buyer.publicRecipient: true`
og `buyer.eanNumber` (13 cifre) i faktura-payloaden.

---

## 11. Begrænsninger

Kendte ting at være opmærksom på, når arbejdet føres videre:

- **Ingen produktions-transmitter eller CLI endnu.** `transmitPublicEInvoicePeppol`
  er testet, men har ingen produktions-kalder, før afsnit 6–7 er gjort.
- **Schematron ikke i CI.** Dokumentet er strukturelt korrekt efter EN16931's
  obligatoriske model, men er endnu ikke kørt mod den officielle schematron.
- **Felt-model-forenklinger:** `seller.vatOrCvr` bruges til både
  `PartyTaxScheme/CompanyID` (BT-31, moms-id) og `PartyLegalEntity/CompanyID`
  (BT-30, CVR), selvom de strengt taget er forskellige. Postadressen er en
  enkelt fritekstlinje + landekode (DK hardkodet, korrekt for denne
  offentlige-modtager-eksport). Det er acceptabelt for simple danske fakturaer,
  men kræver en udvidet datamodel for fuld dækning.
- **To OIOUBL-eksport-events pr. transmission.** `transmitPublicEInvoicePeppol`
  og det interne `submitPublicEInvoicePeppol` genererer begge OIOUBL'en — to
  identiske, deterministiske `public_einvoice_oioubl_export`-events. Harmløst,
  men kan ryddes op ved at lade submit modtage en færdigbygget OIOUBL.
- **Navngivning:** funktioner og CLI-kommandoer hedder stadig "OioUbl"
  (`exportPublicEInvoiceOioUbl`, `invoice export-public-oioubl`), selvom
  outputtet er Peppol BIS Billing 3.0. Bevidst valg for interface-stabilitet —
  en omdøbning er en separat, breaking ændring.

---

## 12. Referencer

- NemHandel — overgang til eDelivery: <https://nemhandel.dk/nemhandel-overgaar-til-edelivery>
- Referenceimplementering v2.0 (Oxalis NG): <https://nemhandel.dk/frigivelse-af-nemhandel-referenceimplementering-v20-den-10-april-2025>
- Oxalis-repo (Erhvervsstyrelsens GitLab): <https://git.erst.dk/openebusiness/nemhandeledelivery/oxalis>
- Registrering af eDelivery-endepunkter i NHR (vejledning, PDF): <https://git.erst.dk/openebusiness/common/-/raw/master/guidelines/vejledning_registrering_nemhandel_edelivery_endpunkter_NHR_WEB_PORS.pdf>
- NemHandel eDelivery demo-endepunkt: <https://nemhandel.dk/nemhandel-edelivery-demo-endepunkt>
- NemHandel — vejledninger og guides: <https://nemhandel.dk/vejledninger-og-guides>
- Peppol BIS Billing 3.0: <https://docs.peppol.eu/poacc/billing/3.0/>
- Erhvervsstyrelsen — NemHandel: <https://erhvervsstyrelsen.dk/nemhandel-faelles-digital-infrastruktur>
