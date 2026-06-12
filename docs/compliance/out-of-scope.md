# Bevidste fravalg — hvad Rentemester ikke dækker

Compliance-matricen i [requirements.md](requirements.md) lister hvad
Rentemester gør for at opfylde dansk bogførings-, moms-, rente-,
betalings- og årsregnskabsret. Denne fil dækker det modsatte:
**hvilke regelsæt Rentemester med vilje ikke håndterer**, og hvorfor.

Det vigtige er, at en bruger ikke skal antage at Rentemester også
løser fx løn, hvidvask-overvågning eller selskabsregistrering. For
nogle af disse fravalg er der en konkret anden løsning. For andre er
fravalget en aktiv arkitektur-beslutning (fx at forblive
ikke-registreret bogføringssystem).

Indeks:

- [§ 1. Det største fravalg: ikke-registreret bogføringssystem](#1-det-største-fravalg-ikke-registreret-bogføringssystem)
- [§ 2. Løn, skat og personalia](#2-løn-skat-og-personalia)
- [§ 3. Selskabsret og virksomhedsadministration](#3-selskabsret-og-virksomhedsadministration)
- [§ 4. Hvidvask, finansiel regulering og overvågning](#4-hvidvask-finansiel-regulering-og-overvågning)
- [§ 5. Cybersikkerhed og platformpligter](#5-cybersikkerhed-og-platformpligter)
- [§ 6. Forbruger- og markedsføringsret](#6-forbruger--og-markedsføringsret)
- [§ 7. Sektor-specifik regulering](#7-sektor-specifik-regulering)
- [§ 8. Fremtidige forpligtelser der ikke er gennemført endnu](#8-fremtidige-forpligtelser-der-ikke-er-gennemført-endnu)
- [§ 9. Sådan beder du om et område der ikke er listet](#9-sådan-beder-du-om-et-område-der-ikke-er-listet)

---

## 1. Det største fravalg: ikke-registreret bogføringssystem

**Bogføringslovens § 15-16** beskriver et **registreret** digitalt
bogføringssystem — typisk en kommerciel SaaS-leverandør (Dinero, Billy,
e-conomic) der har gennemgået Erhvervsstyrelsens registreringsproces og
overholder en strammere bekendtgørelse ([BEK 97/2023 om digitale
standardbogføringssystemer](https://www.retsinformation.dk/eli/lta/2023/97)).

**Rentemester er bevidst ikke-registreret.** Det er forankret i:

- [`sources/legal-sources.json`](../../sources/legal-sources.json) hvor
  `DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205` står som den primære
  reguleringskilde.
- Alle backup-, eksport- og restore-regler i `rules/dk/bookkeeping.yaml`
  citerer den ikke-registrerede bekendtgørelse, ikke standard-bek'en.
- README-prosaen om Rentemester som "agent-first dansk bogholderi".

**Hvad det betyder konkret:**

| Område | Ikke-registreret (Rentemester) | Registreret system |
|---|---|---|
| Backup-tredjepart i EU/EØS | Krav per BEK 205/2024 § 4 stk. 2 | Krav + udbyderen står typisk for det |
| Erhvervsstyrelsen-registrering | Ikke nødvendig | Krav, med vedligeholdelses-pligt |
| Krav til vendor (udbyderen) | Ingen — du er din egen vendor | Skarpe BEK 97/2023-krav |
| Egnet til | Egen bogføring + agent-drift | Bogføring som ydelse til andre |

Vil Rentemester en dag drives som registreret system, skal hele
compliance-profilen revurderes. Indtil da: ikke-registreret er den
aktive klasse, og det er det compliance-matricen er bygget på.

## 2. Løn, skat og personalia

Rentemester håndterer **ikke** løn, skattetræk eller personale-relaterede
forpligtelser. Det dækker disse regelsæt — alle skal løses i andre
systemer:

| Område | Lovkilde | Hvorfor ikke i Rentemester | Hvor det typisk løses |
|---|---|---|---|
| **Lønudbetaling, A-skat, AM-bidrag** | Kildeskatteloven (LBK 1196/2024), arbejdsmarkedsbidragsloven (LBK 121/2020) | Løn kræver eIndkomst-indberetning, ATP/feriepenge-håndtering, ferie-administration. Det er sin egen disciplin. | Danløn, Lessor, Visma, Bluegarden, Salary.dk |
| **Ferielov** | Ferieloven (LBK 1015/2022) | Feriepenge-akkumulering og -udbetaling, samtidighedsferie, hensættelser pr. medarbejder. | Lønsystemet ovenfor |
| **Funktionærloven, ansættelsesbeviser** | Funktionærloven (LBK 1002/2017), ansættelsesbevisloven (LOV 1230/2022) | HR-administration, kontrakter, prøvetid, opsigelser. | HR-system, advokat |
| **Selskabsskat (CIT)** | Selskabsskatteloven (LBK 1241/2021) | Skattegrundlag forberedes deterministisk i Rentemester (jf. `DK-TAX-RETURN-CORP-001`), men selve indberetningen til TastSelv Erhverv sker udenfor. | Brugeren eller dennes revisor i TastSelv Erhverv |
| **Personlig indkomstskat** | Personskatteloven (LBK 1284/2022) | Enkeltmandsvirksomheders skattegrundlag overlapper med selskabsskat-flowet; Rentemester forbereder bilag, brugeren angiver. | TastSelv personlig |
| **Pensionsadministration** | Pensionsbeskatningsloven (LBK 575/2022) | Arbejdsgiver-pensionsbidrag, fradrag, indberetning. | Pensionssselskab, lønsystem |

**Rentemester forbereder skattegrundlag** (jf.
[DK-TAX-RETURN-CORP-001](requirements.md#39-årsregnskabsloven-lbk-11402024)
— advisory severity). Selve indsendelsen sker eksternt.

## 3. Selskabsret og virksomhedsadministration

| Område | Lovkilde | Hvorfor ikke i Rentemester | Hvor det typisk løses |
|---|---|---|---|
| **CVR-registrering, branchekode, P-numre** | Lov om Det Centrale Virksomhedsregister (LBK 568/2022) | Stamdata om virksomheden selv. Rentemester læser CVR-data (via [CVR-API](https://datacvr.virk.dk/data/cvr-help/cvr-api-help)) til kunde-/leverandør-opslag, men registrerer ikke selskaber. | virk.dk |
| **Aktie-/anpartsselskabslov** | Selskabsloven (LBK 1168/2023) | Bestyrelses-/direktions-administration, ejerregister, kapitalforhold, generalforsamlinger. | Selskabsadvokat, virk.dk |
| **Aktionær-/ejerregister** | Selskabsloven §§ 50-58 | Ejer-rapportering til Det Offentlige Ejerregister er en juridisk handling, ikke et bogføringsspørgsmål. | virk.dk |
| **Indberetning af årsrapport** | Årsregnskabsloven § 138 | Rentemester producerer iXBRL (jf. `DK-ANNUAL-REPORT-IXBRL-002`), men selve uploaden til Erhvervsstyrelsen sker udenfor. | regnskab.virk.dk |
| **Stiftelse, fusioner, spaltninger** | Selskabsloven | Engang-juridiske transaktioner. | Advokat |

## 4. Hvidvask, finansiel regulering og overvågning

| Område | Lovkilde | Hvorfor ikke i Rentemester | Hvem skal håndtere det |
|---|---|---|---|
| **Hvidvaskloven (AML)** | Hvidvaskloven (LBK 380/2024) | Hvidvasklovens forpligtelser (kundekendskab, transaktionsovervågning, indberetning til Hvidvasksekretariatet) rammer **finansielle institutioner, ejendomsmæglere, revisorer, advokater og bogføringsvirksomheder der bogfører for andre** — ikke en virksomhed der bogfører sin egen drift. Hvis du driver Rentemester som regnskabs-/bogføringsvirksomhed for kunder, gælder hvidvaskloven dig — som ekstern pligt, ikke som Rentemester-funktion. | Brugeren selv (hvis hvidvaskpligtig); Hvidvasksekretariatet hos NSK |
| **PSD2 / open banking** | PSD2 (direktiv 2015/2366), betalingsloven (LBK 2710/2021) | Adgang til bank-data via PSD2 er en separat infrastruktur. Rentemester konsumerer i dag bank-data via CSV/eksport, ikke via PSD2 API. Se også [docs/psd2-assessment.md](../psd2-assessment.md). | Bank-API leverandør (Tink, Aiia, Nordigen) |
| **Lov om finansiel virksomhed** | FIL (LBK 1731/2024) | Rentemester er ikke en finansiel virksomhed. | — (irrelevant for selvbogføring) |

## 5. Cybersikkerhed og platformpligter

| Område | Lovkilde | Hvorfor ikke i Rentemester | Hvem skal håndtere det |
|---|---|---|---|
| **NIS2-direktivet** | NIS2 (direktiv 2022/2555), implementering kommende | Rammer **væsentlige og vigtige enheder** i specifikke sektorer (energi, transport, sundhed, finans, digital infrastruktur, offentlig forvaltning) — ikke en almindelig SMB der bogfører sin egen drift. Hvis du driver i en NIS2-omfattet sektor, gælder direktivet dig som driftsmæssig pligt. | Virksomhedens IT-/sikkerhedsfunktion |
| **DAC7 platform-rapportering** | DAC7 (direktiv 2021/514), implementeret i skatteindberetningsloven | Rammer **online platforme der formidler transaktioner mellem brugere** (markedspladser, freelance-platforme, udlejningstjenester). En almindelig virksomhed der sælger sine egne ydelser er ikke en platform. | Platform-operatøren, ikke kunden |

## 6. Forbruger- og markedsføringsret

Disse regler er relevante for hvordan en virksomhed sælger til
forbrugere, ikke for hvordan den bogfører. Rentemester arbejder på
det bogføringsmæssige resultat af et salg, ikke på hvordan salget
blev gennemført.

| Område | Lovkilde | Hvorfor ikke i Rentemester |
|---|---|---|
| **Forbrugeraftaleloven** | LBK 1457/2013 | Fortrydelsesret, oplysningspligt ved aftaleindgåelse. Hører til webshop-systemet, ikke bogføringen. |
| **Markedsføringsloven** | LBK 866/2017 | Reklame, sammenligning, vildledende oplysninger. |
| **E-handelsloven** | LBK 227/2002 | Informationspligt for tjenesteudbydere på nettet. |
| **Pakkerejseloven** | LOV 1666/2017 | Rejsearrangører. |

## 7. Sektor-specifik regulering

Rentemester er **branche-uafhængig** og dækker ikke regulering der kun
gælder enkelte erhverv:

- **Apoteksloven, lægemiddelloven** — sundhedssektoren.
- **Spilleloven** — kasinoer, online spil.
- **Tobaks-/alkohol-loven** — punktafgifter (Rentemester håndterer
  generel moms, ikke punktafgifter).
- **Energiafgifter** — el-, gas-, varmeforsyningsloven.
- **Toldloven** — import/eksport-told (Rentemester håndterer EU-moms
  og OSS, ikke told).
- **Landbrugsstøtteordninger, EU-tilskud** — Landbrugsstyrelsens
  systemer.

For en virksomhed i en af de regulerede sektorer skal Rentemester
suppleres af branche-specifikke værktøjer.

## 8. Fremtidige forpligtelser der ikke er gennemført endnu

Ting der **kommer**, men endnu ikke er aktive eller endnu ikke er
implementeret i Rentemester:

| Krav | Forventet ikraftrædelse | Status |
|---|---|---|
| **ViDA — VAT in the Digital Age** | Trinvist 2025-2035 | EU's e-faktura B2B-krav. Rentemester sender allerede OIOUBL/PEPPOL til offentlige modtagere ([DK-PEPPOL-SUBMIT-001](requirements.md#311-lov-om-offentlige-betalinger-lbk-7982007)) men har endnu ikke B2B-PEPPOL flow. |
| **SAF-T fuld decken** | Trinvist 2025-2027 | SAF-T eksport ([DK-BOOKKEEPING-SAFT-EXPORT-001](requirements.md#33-bek-972023--digitale-standardbogføringssystemer-benchmark)) er implementeret som "første slice"; resten af MasterFiles- og GeneralLedgerEntries-blokken implementeres trinvist. |
| **BEK 302/2025 ændring af bilag-opbevaring** | 2026-01-01 | Source er registreret i `legal-sources.json`. Ændringsteksten skal verificeres mod eksisterende regler i [requirements.md § 3.4](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) — TODO. |

## 9. Sådan beder du om et område der ikke er listet

Hvis du støder på en compliance-disciplin Rentemester ikke dækker, og
du vurderer at den **bør** dækkes:

1. Tjek om den er listet her som et bevidst fravalg. Hvis ja, og du er
   uenig i fravalget, så er det et arkitektur-spørgsmål — åbn et issue
   med begrundelse for hvorfor scope skal udvides.
2. Hvis den ikke er listet — hverken her eller i [requirements.md](requirements.md)
   — så er det enten et **hul** (compliance der bør være kodificeret
   men ikke er) eller et **uudforsket område** (en ny disciplin vi ikke
   har overvejet endnu).
3. I begge tilfælde: åbn et issue med (a) lovkilden, (b) den konkrete
   forpligtelse, (c) hvilken del af Rentemester den ville røre. Brug
   compliance-audit-skabelonen i `docs/compliance/` (kommer i en
   senere PR) eller skriv frit.

Sletninger fra denne liste er lige så vigtige som tilføjelser: hvis et
fravalg ikke længere er bevidst (fx fordi du har valgt at registrere
Rentemester som standardbogføringssystem), skal listen opdateres samme
PR som scope-ændringen.
