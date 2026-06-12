# Compliance-krav grupperet efter system-område

Samme regelsæt som [requirements.md](requirements.md), men set fra
modsat retning: i stedet for at gå fra lov → regel → kode, går vi her
fra **det system-område du arbejder i** → de regler der gælder her →
deres lovkilde.

Brug denne fil når du:

- Skal lave en ændring i en konkret del af Rentemester og vil vide
  hvilke compliance-krav der rammer den.
- Skal forstå hvad et område *som hele* skal leve op til, før du dykker
  i kildekoden.
- Skal forklare en ekstern (revisor, jurist, ny udvikler), hvad et
  bestemt område af systemet skal kunne.

Den anden retning (lov → kode) er den primære reference og findes i
[requirements.md](requirements.md). Begge filer holdes synkroniserede.

Indeks:

- [§ 1. Hovedbog & journal-integritet](#1-hovedbog--journal-integritet)
- [§ 2. Bilag og dokumenter](#2-bilag-og-dokumenter)
- [§ 3. Bilags-indtagelse (mail intake)](#3-bilags-indtagelse-mail-intake)
- [§ 4. Salgsfakturaer — udstedelse](#4-salgsfakturaer--udstedelse)
- [§ 5. Salgsfakturaer — livscyklus (betaling, kredit, refund)](#5-salgsfakturaer--livscyklus-betaling-kredit-refund)
- [§ 6. Sen-betalingsregler (rente, gebyr, kompensation)](#6-sen-betalingsregler-rente-gebyr-kompensation)
- [§ 7. Indkøbsfakturaer & kreditorer](#7-indkøbsfakturaer--kreditorer)
- [§ 8. Bank og afstemning](#8-bank-og-afstemning)
- [§ 9. Moms](#9-moms)
- [§ 10. Anlægsaktiver og afskrivninger](#10-anlægsaktiver-og-afskrivninger)
- [§ 11. Periodeafgrænsning (accruals)](#11-periodeafgrænsning-accruals)
- [§ 12. Periode-låsning](#12-periode-låsning)
- [§ 13. Backup og restore](#13-backup-og-restore)
- [§ 14. Opbevaring (retention) og GDPR](#14-opbevaring-retention-og-gdpr)
- [§ 15. Myndighedsudlevering og SAF-T](#15-myndighedsudlevering-og-saf-t)
- [§ 16. Public e-faktura (NemHandel / OIOUBL / PEPPOL)](#16-public-e-faktura-nemhandel--oioubl--peppol)
- [§ 17. Årsrapport og skat](#17-årsrapport-og-skat)
- [§ 18. Master data (kunder, leverandører)](#18-master-data-kunder-leverandører)
- [§ 19. Kørselsregnskab (mileage)](#19-kørselsregnskab-mileage)
- [§ 20. Email-afsendelse](#20-email-afsendelse)
- [§ 21. Recurring invoices](#21-recurring-invoices)

---

## 1. Hovedbog & journal-integritet

**Hvad området skal kunne:** Fungere som en append-only kassebog hvor
hver postering balancerer, har et bilag, og hvor rettelser kun sker via
nye modposterings-entries — ikke ved at ændre originaler.

**Lovkilder:** Bogføringsloven §§ 7-9, 13.

**Centrale filer:** `src/core/ledger.ts`, `src/core/accruals.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-BALANCED-001](requirements.md#31-bogføringsloven-lov-7002022) | Debet = kredit i hver postering. |
| [DK-BOOKKEEPING-APPEND-ONLY-001](requirements.md#31-bogføringsloven-lov-7002022) | Ingen UPDATE/DELETE på journalposter — kun nye entries. |
| [DK-BOOKKEEPING-REVERSAL-001](requirements.md#31-bogføringsloven-lov-7002022) | Rettelser sker som én sporbar modposterings-entry. |
| [DK-BOOKKEEPING-DOCUMENT-001](requirements.md#31-bogføringsloven-lov-7002022) | Bilag tilknyttet posteringer hvor det er relevant. |

Disse fire er **kode-håndhævet**: databaseskemaet og repository-laget
tillader ikke andet. Der findes ingen guide.

---

## 2. Bilag og dokumenter

**Hvad området skal kunne:** Tage imod digitale købs- og salgsbilag,
tjekke at de bærer de statutoriske minimumsfelter, bevare dem
indholds-adresseret (sha256) og uden mutation.

**Lovkilder:** BEK 1383/2023 (bilag-opbevaring) + bogføringsloven § 9, § 13.

**Centrale filer:** `src/core/documents.ts`, `src/core/master-data.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-DOCUMENT-STORAGE-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | 7 minimumsfelter på fuldt bilag (afsender, modtager, dato, beløb, moms, betalingsdetaljer, leveringsbeskrivelse). |
| [DK-DOCUMENT-CASH-RECEIPT-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Kassestrimler — original-fil + valuta bevares, fuldt sæt undtaget. |
| [DK-DOCUMENT-FOREIGN-PHYSICAL-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Fremmedlandske fysiske bilag — original + valuta bevares. |
| [DK-DOCUMENT-INTEGRITY-001](requirements.md#31-bogføringsloven-lov-7002022) | sha256 content-addressed, ingen silent overwrite, append-only audit. |
| [DK-MASTER-DATA-VENDOR-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Leverandør-stamdata opløses deterministisk og materialiseres ind i bilagets metadata. |

Alle er kode-håndhævet.

---

## 3. Bilags-indtagelse (mail intake)

**Hvad området skal kunne:** Læse bilag fra en lokal maildrop eller EML-fil
ind i systemet, deduplikere, og route det utvetydige til exception-køen
i stedet for at gætte.

**Lovkilder:** BEK 1383/2023 § 1, stk. 1, samt bogføringsloven §§ 7, 9.

**Centrale filer:** `src/core/mail-intake.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-MAIL-INTAKE-TRANSPORT-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Maildrop/EML, ikke hostet mailbox (advisory). |
| [DK-MAIL-INTAKE-DEDUP-001](requirements.md#31-bogføringsloven-lov-7002022) | Dedup på message-id + attachment-hash. |
| [DK-MAIL-INTAKE-EXCEPTION-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Tvetydige beskeder til exception-kø — menneske afgør. |

`EXCEPTION-001` er **menneske-attesteret** i sin natur — systemet kan ikke
gætte sig til det rigtige bilag.

---

## 4. Salgsfakturaer — udstedelse

**Hvad området skal kunne:** Producere fakturaer der opfylder
momsbekendtgørelsens minimumssæt før de udstedes, og som derefter er
immutable.

**Lovkilder:** Momsbekendtgørelsen §§ 58-66, momsloven § 23.

**Centrale filer:** `src/core/invoice.ts`, `src/core/invoice-pdf.ts`,
`src/core/issued-invoices.ts`, `src/core/master-data.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-INVOICE-FULL-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | 9 minimumsfelter på fuld faktura. |
| [DK-INVOICE-SIMPLIFIED-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Forenklet faktura kun op til DKK 3.000 brutto. |
| [DK-INVOICE-REVERSE-CHARGE-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Reverse-charge faktura uden moms-beløb + med note. |
| [DK-INVOICE-REVERSE-CHARGE-BASIS-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Specifik lovbestemmelse som fritagelsesgrundlag. |
| [DK-INVOICE-DELIVERY-DATE-001](requirements.md#35-momsloven-lbk-2092024) | Leveringsdato hvis ≠ udstedelsesdato. |
| [DK-INVOICE-ARITHMETIC-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Linjesummer + totaler stemmer aritmetisk. |
| [DK-INVOICE-ISSUE-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Sekventielle fakturanumre, immutabel lagring. |
| [DK-INVOICE-LOCK-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Udstedte fakturaer kan ikke ændres. |
| [DK-MASTER-DATA-CUSTOMER-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Kunde-stamdata opløses deterministisk før udstedelse. |
| [DK-VAT-SEPARATE-AMOUNT-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Dansk moms-fradrag kræver angivet moms-beløb. |

Alle kode-håndhævet.

---

## 5. Salgsfakturaer — livscyklus (betaling, kredit, refund)

**Hvad området skal kunne:** Følge fakturaen fra udstedelse til betaling,
kreditnota, eller endelig afskrivning — altid med sporbarhed til
banktransaktioner og linkede dokumenter.

**Lovkilder:** Momsbekendtgørelsen §§ 58, 66 + bogføringsloven § 9.

**Centrale filer:** `src/core/invoice-payments.ts`,
`src/core/invoice-settlement.ts`, `src/core/credit-notes.ts`,
`src/core/invoice-refunds.ts`, `src/core/invoice-bad-debt.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-INVOICE-PAYMENT-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Betalinger anvendes sporbart mod faktura + bank. |
| [DK-INVOICE-SETTLEMENT-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Bank-modtagne kunde-betalinger udligner tilgodehavender. |
| [DK-INVOICE-COMBINED-SETTLEMENT-001](requirements.md#31-bogføringsloven-lov-7002022) | Én bank-modtagelse kan udligne hovedstol + krav. |
| [DK-INVOICE-CORRECTION-BALANCE-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Åben saldo afspejler kreditnotaer såvel som betalinger. |
| [DK-CREDIT-NOTE-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Kreditnota refererer original + spejler moms-effekt. |
| [DK-INVOICE-REFUND-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Kunde-refunderinger sporbart mod bank + korrigeret saldo. |
| [DK-INVOICE-BAD-DEBT-WRITEOFF-001](requirements.md#31-bogføringsloven-lov-7002022) | Uerholdelige tilgodehavender via append-only korrektion. |
| [DK-VAT-BAD-DEBT-001](requirements.md#35-momsloven-lbk-2092024) | Bad-debt reducerer output-moms via 80 %-tabsgrundlag. |
| [DK-INVOICE-BOOKKEEPING-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Én sporbar tilgodehavende-, omsætnings- og moms-postering. |
| [DK-INVOICE-BOOKKEEPING-REVERSE-002](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Reverse-charge bogføring uden output-moms-linje. |

Alle kode-håndhævet.

---

## 6. Sen-betalingsregler (rente, gebyr, kompensation)

**Hvad området skal kunne:** Beregne deterministisk hvad en kunde skylder
ud over hovedstol når en faktura er overdue: morarente, rykkergebyrer
og DKK 310-kompensation (B2B). Hver registrering må kun ske én gang og
skal forblive sporbar.

**Lovkilder:** Renteloven §§ 3, 5, 9b + BEK 105/2013 (DKK 310-kompensation).

**Centrale filer:** `src/core/invoice-interest.ts`,
`src/core/invoice-reminders.ts`, `src/core/invoice-compensation.ts`,
`src/core/invoice-payments.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-INVOICE-DUE-DATE-001](requirements.md#37-renteloven-lbk-4592014) | Deterministisk forfaldsdato + overdue-klassifikation. |
| [DK-INVOICE-LATE-INTEREST-001](requirements.md#37-renteloven-lbk-4592014) | Statutorisk morarenteberegning på overdue. |
| [DK-INVOICE-LATE-INTEREST-REGISTER-001](requirements.md#37-renteloven-lbk-4592014) | Krav registreres kun fra deterministisk beregning. |
| [DK-INVOICE-LATE-INTEREST-BOOKKEEPING-001](requirements.md#31-bogføringsloven-lov-7002022) | Bogføring én gang til tilgodehavender + krav-indtægt. |
| [DK-INVOICE-REMINDER-FEE-001](requirements.md#37-renteloven-lbk-4592014) | Maxbeløb, max-antal og 10-dages spacing. |
| [DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001](requirements.md#31-bogføringsloven-lov-7002022) | Bogføring én gang til tilgodehavender + krav-indtægt. |
| [DK-INVOICE-LATE-COMPENSATION-001](requirements.md#38-bek-1052013--udenretlige-inddrivelsesomkostninger-kompensation) | Vurdering af det faste DKK 310-krav. |
| [DK-INVOICE-LATE-COMPENSATION-REGISTER-001](requirements.md#38-bek-1052013--udenretlige-inddrivelsesomkostninger-kompensation) | Registreres kun én gang pr. overdue erhvervsfaktura. |
| [DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001](requirements.md#31-bogføringsloven-lov-7002022) | Bogføring én gang til tilgodehavender + krav-indtægt. |
| [DK-INVOICE-CLAIM-SETTLEMENT-001](requirements.md#31-bogføringsloven-lov-7002022) | Bank-modtagne krav-indbetalinger udligner sporbart. |

Alle kode-håndhævet.

---

## 7. Indkøbsfakturaer & kreditorer

**Hvad området skal kunne:** Registrere indkomne leverandørfakturaer som
balancerede åbne kreditorposter, og udligne dem mod udgående bank-
transaktioner.

**Lovkilder:** Bogføringsloven §§ 7, 9.

**Centrale filer:** `src/core/payables.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-PAYABLE-001](requirements.md#31-bogføringsloven-lov-7002022) | Sporbar, balanceret åben kreditorpost. |
| [DK-PAYABLE-PAYMENT-001](requirements.md#31-bogføringsloven-lov-7002022) | Betalinger sporbart mod kreditorpost + bank. |

Alle kode-håndhævet.

---

## 8. Bank og afstemning

**Hvad området skal kunne:** Importere bankudtog uden tab af originaldata,
tjekke saldokontinuitet, og afstemme periodiske transaktioner mod
hovedbogen.

**Lovkilder:** Bogføringsloven § 7, § 9, § 11.

**Centrale filer:** `src/core/bank.ts`, `src/core/reconciliation.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-BANK-IMPORT-001](requirements.md#31-bogføringsloven-lov-7002022) | Dato, beløb, tekst, sporbar batch bevares. |
| [DK-BANK-BALANCE-CONTINUITY-001](requirements.md#31-bogføringsloven-lov-7002022) | Kontinuitet i løbende saldo tjekkes (warning). |
| [DK-BOOKKEEPING-RECONCILIATION-001](requirements.md#31-bogføringsloven-lov-7002022) | Matchede + umatchede transaktioner i en periode. |
| [DK-BOOKKEEPING-FX-001](requirements.md#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer) | Ikke-DKK: valuta + vekselfaktor + DKK-beløb. |

Alle kode-håndhævet.

---

## 9. Moms

**Hvad området skal kunne:** Sammenstille et periode-grundlag for moms,
booke EU-reverse-charge korrekt, anvende den 25 %-grænse for
repræsentation, klargøre den indberetnings-klare angivelse, og holde
EU-salg-uden-moms + OSS adskilt fra standard-angivelsen.

**Lovkilder:** Momsloven §§ 23, 27, 42, 46, 54, 56, 57 + momsbekendtgørelsen § 58.

**Centrale filer:** `src/core/vat.ts`, `src/core/vat-filing.ts`,
`src/core/vat-vies-list.ts`, `src/core/vat-oss.ts`, `src/core/vies.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-VAT-REPORT-001](requirements.md#35-momsloven-lbk-2092024) | Output- + input-moms + netto-skyld pr. periode. |
| [DK-VAT-REVERSE-CHARGE-001](requirements.md#35-momsloven-lbk-2092024) | EU-ydelseskøb: både output + fradragsberettiget input. |
| [DK-VAT-REPRESENTATION-001](requirements.md#35-momsloven-lbk-2092024) | Repræsentation: 25 % af momsen er fradragsberettiget. |
| [DK-VAT-BAD-DEBT-001](requirements.md#35-momsloven-lbk-2092024) | Bad-debt reducerer output-moms via 80 %-tabsgrundlag. |
| [DK-VAT-FILING-001](requirements.md#35-momsloven-lbk-2092024) | Klar momsangivelse kun fra lukket periode, mappet til SKAT-rubrikker. |
| [DK-VAT-EU-SALES-LIST-001](requirements.md#35-momsloven-lbk-2092024) | EU B2B-salg uden DK-moms grupperes pr. kunde-VAT. |
| [DK-VAT-OSS-001](requirements.md#35-momsloven-lbk-2092024) | OSS-salg holdes ude af standard-angivelsen. |
| [DK-VAT-SEPARATE-AMOUNT-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Dansk moms-fradrag kræver angivet moms-beløb. |

Alle kode-håndhævet. VIES-lookup mod EU's moms-VIES-tjeneste bruges
til at validere modparts-VAT før EU-bogføring — det er et eksternt
API-kald, men selve compliance-spørgsmålet (er VAT-nummeret gyldigt
ved bogføringstidspunktet?) er kode-håndhævet via cachen.

---

## 10. Anlægsaktiver og afskrivninger

**Hvad området skal kunne:** Aktivere et anlægsaktiv ved køb og poste
deterministiske, balancerede afskrivnings-entries der forbliver linket
til den oprindelige købs-dokumentation. Straks-afskrivning af
småanskaffelser kræver eksplicit menneske-bekræftelse og en kildebakket
tærskel-regel.

**Lovkilder:** Bogføringsloven §§ 7, 9 (generelt) + selskabsskattelovens
afskrivningsregler (uden for Rentemester-domænet — brugeren/revisoren
ejer skattevalget).

**Centrale filer:** `src/core/assets.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-ASSET-DEPR-001](requirements.md#31-bogføringsloven-lov-7002022) | Deterministisk lineær afskrivningsplan, balanceret pr. periode, linket til købet. |
| [DK-ASSET-WRITEOFF-001](requirements.md#31-bogføringsloven-lov-7002022) | Straks-afskrivning kræver eksplicit confirmation + kildebakket regel-metadata. |

`ASSET-WRITEOFF-001` er **hybrid**: koden tjekker at confirmation er
sat og at threshold-reglen har en kilde — men selve valget om straks-
afskrivning sker hos brugeren.

---

## 11. Periodeafgrænsning (accruals)

**Hvad området skal kunne:** Parkere en forudbetaling eller en deferred
revenue på en balancekonto med én balanceret postering, og indregne
den deterministisk over flere perioder så summen stemmer eksakt
(med øre-rest i sidste periode).

**Lovkilder:** Bogføringsloven §§ 7, 9.

**Centrale filer:** `src/core/accruals.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-ACCRUAL-001](requirements.md#31-bogføringsloven-lov-7002022) | Balanceret park-postering + deterministiske periode-entries der summer eksakt. |

Kode-håndhævet.

---

## 12. Periode-låsning

**Hvad området skal kunne:** Forhindre posteringer der rammer lukkede
regnskabsperioder, og forhindre fremtidsdaterede journalposteringer.

**Lovkilder:** BEK 205/2024 § 3, stk. 3.

**Centrale filer:** `src/core/periods.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-PERIOD-LOCK-001](requirements.md#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer) | Ingen mutation i lukket periode, ingen fremtidsdatering. |

Kode-håndhævet.

---

## 13. Backup og restore

**Hvad området skal kunne:** Tage en fuld ugentlig backup af bogførte
transaktioner + bilag, lægge den hos en uafhængig tredjepart på en
server i et EU/EØS-land, og kunne restore den til et læsbart
selskabs-dataset.

**Lovkilder:** BEK 205/2024 § 4 + BEK 97/2023 § 10.

**Centrale filer:** `src/core/system-backups.ts`,
`src/core/backup-governance.ts`, `src/core/system-restore.ts`,
`src/core/backup-guide.ts`.

| Rule | Hvad den dækker | Karakter |
|---|---|---|
| [DK-BOOKKEEPING-BACKUP-001](requirements.md#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer) | Ugentlig fuld backup med manifest + hashes. | hybrid |
| [DK-BOOKKEEPING-BACKUP-DEST-001](requirements.md#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer) | EU/EØS-server, ikke-nærtstående, formodnings-egnede it-sikkerhedsstandarder. | **menneske** |
| [DK-BOOKKEEPING-BACKUP-KEY-ROTATE-001](requirements.md#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer) | Auditerbar rotation af Ed25519-signaturnøgle. | menneske |
| [DK-BOOKKEEPING-RESTORE-001](requirements.md#33-bek-972023--digitale-standardbogføringssystemer-benchmark) | Restore til læsbart dataset, manifest- + hash-verifikation først. | kode |

**Guides for de menneskelige dele:**

- [docs/compliance/backup-destinations.md](backup-destinations.md) —
  master, hvor og hvordan en destination attesteres.
- [docs/compliance/backup-destinations/google-workspace.md](backup-destinations/google-workspace.md) — konkret opskrift for Workspace med Data Regions = Europa.
- [docs/compliance/backup-destinations/_TEMPLATE.md](backup-destinations/_TEMPLATE.md) — skabelon for nye udbydere.
- [docs/backup-security.md](../backup-security.md) — chain-of-trust for signing-nøglen.

---

## 14. Opbevaring (retention) og GDPR

**Hvad området skal kunne:** Tagge alt regnskabsmateriale med en
deterministisk `retain_until` = regnskabsårets udgang + 5 år. Give et
registreret subjekt indsigt i sine persondata (GDPR art. 15) og afvise
sletning af regnskabsmateriale inden for retention-vinduet (GDPR
art. 17, stk. 3, lit. b og e — bogføringspligten som retlig
forpligtelse).

**Lovkilder:** Bogføringsloven § 12, stk. 1 + GDPR art. 15, 17.

**Centrale filer:** `src/core/retention.ts`, `src/core/gdpr.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-RETENTION-001](requirements.md#31-bogføringsloven-lov-7002022) | `retain_until` på bilag, posteringer, banktransaktioner. |
| [GDPR-SUBJECT-EXPORT](requirements.md#310-gdpr-forordning-2016679) | Indsigt: struktureret eksport af subjektets persondata. |
| [GDPR-RETENTION-BOUNDED-ERASURE](requirements.md#310-gdpr-forordning-2016679) | Sletning: afvises inden for retention-vinduet, logges som tombstone. |

Kode-håndhævet. Selve sletteanmodningen er kunde-initieret (CLI/MCP) og
løsningen blokerer sletning af materiale inden for retention-vinduet.

---

## 15. Myndighedsudlevering og SAF-T

**Hvad området skal kunne:** Pakke en periode-bundet eksport af
bogføringsdata + bilag som maskinlæsbar, deterministisk pakke — både i
det generelle authority-format og som SAF-T XML — så materialet kan
udleveres til myndigheden inden for 1-uges-fristen.

**Lovkilder:** Bogføringsloven §§ 14-15 + BEK 97/2023 § 11.

**Centrale filer:** `src/core/authority-export.ts`, `src/core/saft-export.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-BOOKKEEPING-AUTHORITY-EXPORT-001](requirements.md#33-bek-972023--digitale-standardbogføringssystemer-benchmark) | Periode-bundet, struktureret, maskinlæsbar pakke. |
| [DK-BOOKKEEPING-SAFT-EXPORT-001](requirements.md#33-bek-972023--digitale-standardbogføringssystemer-benchmark) | SAF-T XML med manifest-hashes og eksplicitte out-of-scope-sektioner. |

Alle kode-håndhævet.

---

## 16. Public e-faktura (NemHandel / OIOUBL / PEPPOL)

**Hvad området skal kunne:** Producere en OIOUBL-kompatibel faktura for
offentlige modtagere, eksportere den som handoff-pakke, og — når
PEPPOL-access point er sat op — sende den deterministisk og idempotent.

**Lovkilder:** Lov om offentlige betalinger (LBK 798/2007) — pligten
for offentlige myndigheder til at modtage elektroniske fakturaer.
Faktura-indholdsfelterne for fakturaer generelt sidder i
momsbekendtgørelsen § 58 (se [§ 4. Salgsfakturaer — udstedelse](#4-salgsfakturaer--udstedelse)).
De tekniske formater (OIOUBL, PEPPOL BIS Billing 3) er specificeret i
underliggende bekendtgørelser.

**Centrale filer:** `src/core/public-einvoice.ts`.

| Rule | Hvad den dækker | Karakter |
|---|---|---|
| [DK-INVOICE-PUBLIC-RECIPIENT-001](requirements.md#311-lov-om-offentlige-betalinger-lbk-7982007) | EAN/GLN i immutabel buyer-snapshot. | kode |
| [DK-INVOICE-PUBLIC-EXPORT-001](requirements.md#311-lov-om-offentlige-betalinger-lbk-7982007) | Deterministisk eksport-preview, transport-fri handoff. | kode |
| [DK-INVOICE-PUBLIC-OIOUBL-001](requirements.md#311-lov-om-offentlige-betalinger-lbk-7982007) | OIOUBL-handoff-eksport, transport-bundet. | kode |
| [DK-PEPPOL-SUBMIT-001](requirements.md#311-lov-om-offentlige-betalinger-lbk-7982007) | PEPPOL-indsendelse, deterministisk + idempotent. | **hybrid** |

**Guide:** [docs/peppol-nemhandel.md](../peppol-nemhandel.md) — selv-hostet
Oxalis access point, MitID systemcertifikat, NemHandelsRegister-endpoint-
registrering. PEPPOL-transporten kræver opsætning udenfor Rentemester
(certifikat, endpoint) — den menneskelige del er dækket af guiden.

---

## 17. Årsrapport og skat

**Hvad området skal kunne:** Samle en årsrapport for regnskabsklasse B
fra et fuldt låst regnskabsår, eksportere den som iXBRL i den
deklarerede taksonomi-subset, og forberede selskabsskattepligtig
indkomst deterministisk fra årsrapporten.

**Lovkilder:** Årsregnskabsloven (LBK 1140/2024) — hele.

**Centrale filer:** `src/core/annual-report.ts`, `src/core/ixbrl.ts`,
`src/core/tax-return.ts`.

| Rule | Hvad den dækker | Karakter |
|---|---|---|
| [DK-ANNUAL-REPORT-CLASS-B-001](requirements.md#39-årsregnskabsloven-lbk-11402024) | Årsrapport kun fra fuldt låst regnskabsår + komplette master data. | kode |
| [DK-ANNUAL-REPORT-IXBRL-002](requirements.md#39-årsregnskabsloven-lbk-11402024) | Deterministisk iXBRL i micro/small-taksonomi-subset. | kode |
| [DK-TAX-RETURN-CORP-001](requirements.md#39-årsregnskabsloven-lbk-11402024) | Selskabsskattegrundlag fra låst årsrapport; ikke-deterministiske justeringer som needs-review. | advisory |

Selve indberetningen til Erhvervsstyrelsen er en menneskelig handling —
Rentemester forbereder iXBRL-filen, du og din revisor gennemgår og indberetter.

---

## 18. Master data (kunder, leverandører)

**Hvad området skal kunne:** Holde kunde- og leverandør-stamdata
append-only og sikre at de fields der ender på et udstedt bilag,
opløses deterministisk *før* udstedelse — så et stamdata-skift senere
ikke ændrer historiske bilag.

**Lovkilder:** Momsbekendtgørelsen § 58 (faktura-indhold) +
BEK 1383/2023 § 1 (bilags-indhold).

**Centrale filer:** `src/core/master-data.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-MASTER-DATA-CUSTOMER-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Kunde-stamdata opløses deterministisk før fakturaudstedelse. |
| [DK-MASTER-DATA-VENDOR-001](requirements.md#34-bek-13832023--pligt-til-opbevaring-af-bilag) | Leverandør-stamdata opløses deterministisk + materialiseres i bilags-metadata. |

Alle kode-håndhævet.

---

## 19. Kørselsregnskab (mileage)

**Hvad området skal kunne:** Gemme en kørselslog der er komplet og
kilde-bakket før den lagres.

**Lovkilder:** Bogføringsloven § 9 (generelt).

**Centrale filer:** `src/core/mileage.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-MILEAGE-LOG-001](requirements.md#31-bogføringsloven-lov-7002022) | Komplet kørselslog-post, kilde-bakket før gemning. |

Kode-håndhævet.

---

## 20. Email-afsendelse

**Hvad området skal kunne:** Sende fakturaer og rykkere via SMTP
deterministisk og idempotent, og logge afsendelser append-only.

**Lovkilder:** Bogføringsloven § 9 (bilag-spor).

**Centrale filer:** `src/core/email.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-EMAIL-DELIVERY-001](requirements.md#31-bogføringsloven-lov-7002022) | Deterministisk, idempotent, append-only send-log. |

Kode-håndhævet. SMTP-credentials skal være krypteret i config — det er
ikke et rule_id i sig selv, men en sikkerheds-praksis dokumenteret i
`rentemester.md` § 34.2.

---

## 21. Recurring invoices

**Hvad området skal kunne:** Gemme en præ-valideret faktura-skabelon der
kan generere konkrete fakturaer deterministisk på et givet tidspunkt.

**Lovkilder:** Momsbekendtgørelsen § 58 (forhåndsvalidering).

**Centrale filer:** `src/core/recurring-invoices.ts`.

| Rule | Hvad den dækker |
|---|---|
| [DK-RECURRING-INVOICE-TEMPLATE-001](requirements.md#36-momsbekendtgørelsen-bek-14352023) | Immutabel, præ-valideret faktura-specifikation. |

Genererings-skridtet selv har et internt rule_id `DK-RECURRING-INVOICE-GENERATE-001`, som ikke har en juridisk kilde — det er en kvalitetsregel om at genereringen er deterministisk og idempotent (se [§ 4 i requirements.md](requirements.md#4-interne-ikke-lovbestemte-regler)).
