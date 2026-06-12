# Compliance-matrix

Den komplette liste over alle compliance-krav Rentemester skal opfylde
som **selvudviklet, ikke-registreret digitalt bogføringssystem** efter
dansk ret, mappet ende-til-ende:

```
Lovkilde (LOV/BEK + paragraf)
   ↓                   ↓
Rentemester rule_id    XML / ELI
   ↓                                  ↓
Håndhævelse i kode     (Guide for menneske-attesteret del)
```

Hver række binder en juridisk forpligtelse til en konkret YAML-regel,
en konkret kildefil i `src/`, og — hvor reglen ikke kan løses 100 % i
kode — til en menneske-følgbar guide.

Indeks:

- [§ 1. Sådan læser du matricen](#1-sådan-læser-du-matricen)
- [§ 2. Lovkilder og scope](#2-lovkilder-og-scope)
- [§ 3. Matricen pr. lovkilde](#3-matricen-pr-lovkilde)
  - [3.1 Bogføringsloven (LOV 700/2022)](#31-bogføringsloven-lov-7002022)
  - [3.2 BEK 205/2024 — Ikke-registrerede digitale bogføringssystemer](#32-bek-2052024--ikke-registrerede-digitale-bogføringssystemer)
  - [3.3 BEK 97/2023 — Digitale standardbogføringssystemer (benchmark)](#33-bek-972023--digitale-standardbogføringssystemer-benchmark)
  - [3.4 BEK 1383/2023 — Pligt til opbevaring af bilag](#34-bek-13832023--pligt-til-opbevaring-af-bilag)
  - [3.5 Momsloven (LBK 209/2024)](#35-momsloven-lbk-2092024)
  - [3.6 Momsbekendtgørelsen (BEK 1435/2023)](#36-momsbekendtgørelsen-bek-14352023)
  - [3.7 Renteloven (LBK 459/2014)](#37-renteloven-lbk-4592014)
  - [3.8 BEK 105/2013 — Udenretlige inddrivelsesomkostninger (kompensation)](#38-bek-1052013--udenretlige-inddrivelsesomkostninger-kompensation)
  - [3.9 Årsregnskabsloven (LBK 1140/2024)](#39-årsregnskabsloven-lbk-11402024)
  - [3.10 GDPR (forordning 2016/679)](#310-gdpr-forordning-2016679)
  - [3.11 Lov om offentlige betalinger (LBK 798/2007)](#311-lov-om-offentlige-betalinger-lbk-7982007)
- [§ 4. Interne (ikke-lovbestemte) regler](#4-interne-ikke-lovbestemte-regler)
- [§ 5. Sådan holder du matricen aktuel](#5-sådan-holder-du-matricen-aktuel)

> En anden vinkel på samme krav — grupperet efter system-område og ikke
> efter lovkilde — findes i
> [requirements-by-area.md](requirements-by-area.md).

---

## 1. Sådan læser du matricen

Hver række i de følgende tabeller har samme form:

| Kolonne | Hvad det betyder |
|---|---|
| **Rule** | Rentemester rule_id med direkte link til regel-definitionen i `rules/dk/*.yaml`. |
| **Paragraf** | Den juridiske bestemmelse rule_id'et udspringer af (fx `§ 4, stk. 2`). For lovkilder med kompositte rule_id'er (en rule_id, mange paragrafer) er alle paragrafer listet. |
| **Hvad kræves** | En sætning, der gengiver kernen af reglen — ikke ordret lovtekst, men hvad systemet skal kunne. |
| **Håndhævelse** | Et link til den primære kildefil + linje hvor rule_id-konstanten er deklareret. Det er indgangen til implementationen. |
| **Karakter** | Hvordan reglen rent faktisk opfyldes — se klassifikationen herunder. |
| **Guide** | Link til en menneske-følgbar opskrift (i `docs/compliance/`), hvis reglen kræver en handling uden for koden. |

### Karakter-klassifikation

| Værdi | Hvad det betyder | Eksempel |
|---|---|---|
| **kode** | Reglen håndhæves 100 % i kode. Bruges der systemet rigtigt, kan reglen ikke overtrædes. | Append-only journal (`DK-BOOKKEEPING-APPEND-ONLY-001`) — der findes ingen UPDATE-sti i databasen. |
| **hybrid** | Kode klarer alt det maskinen kan; et menneske skal attestere det maskinen ikke kan vide. | Backup-ugepligt (`DK-BOOKKEEPING-BACKUP-001`) — koden tjekker tidsstempler og hashes, men ikke at destinationen ligger i EU/EØS. |
| **menneske** | Reglen kræver i sin natur en menneskelig handling eller attestation, og koden registrerer den blot. | Ed25519-nøglerotation (`DK-BOOKKEEPING-BACKUP-KEY-ROTATE-001`) — kun et menneske kan beslutte at rotere og forklare hvorfor. |

### Severity-niveauer

YAML-feltet `severity` på hver rule:

- **hard_stop** — overtrædelse blokerer den operation, der ville bryde reglen.
- **warning** — operationen kan udføres, men der rapporteres et compliance-issue.
- **advisory** — best practice; ingen blokering, men logges som anbefaling.

---

## 2. Lovkilder og scope

| Source-ID | Titel | Myndighed | Scope (in_scope) | URL | XML/ELI |
|---|---|---|---|---|---|
| `DK-BOGFORINGSLOVEN-2022-700` | Lov om bogføring | Erhvervsministeriet | §§ 1-18 | [retsinformation](https://www.retsinformation.dk/eli/lta/2022/700) | [xml](https://www.retsinformation.dk/eli/lta/2022/700/xml) |
| `DK-DIGITAL-BOGFORING-NONREGISTERED-2024-205` | BEK om krav til ikke-registrerede digitale bogføringssystemer | Erhvervsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2024/205) | [xml](https://www.retsinformation.dk/eli/lta/2024/205/xml) |
| `DK-DIGITAL-STANDARD-BOGFORING-2023-97` | BEK om krav til digitale standardbogføringssystemer | Erhvervsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2023/97) | [xml](https://www.retsinformation.dk/eli/lta/2023/97/xml) |
| `DK-BILAG-OPBEVARING-2023-1383` | BEK om pligt til opbevaring af bilag i et digitalt bogføringssystem | Erhvervsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2023/1383) | [xml](https://www.retsinformation.dk/eli/lta/2023/1383/xml) |
| `DK-BILAG-OPBEVARING-AMEND-2025-302` | Ændring af bilags-opbevarings-bek (gælder fra 2026-01-01) | Erhvervsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2025/302) | [xml](https://www.retsinformation.dk/eli/lta/2025/302/xml) |
| `DK-MOMSLOVEN-2024-209` | LBK af lov om merværdiafgift (momsloven) | Skatteministeriet | §§ 23, 27, 37-42, 46, 47-57 | [retsinformation](https://www.retsinformation.dk/eli/lta/2024/209) | [xml](https://www.retsinformation.dk/eli/lta/2024/209/xml) |
| `DK-MOMSBEKENDTGORELSEN-2023-1435` | Momsbekendtgørelsen | Skatteministeriet | §§ 58-72 | [retsinformation](https://www.retsinformation.dk/eli/lta/2023/1435) | [xml](https://www.retsinformation.dk/eli/lta/2023/1435/xml) |
| `DK-RENTELOVEN-2014-459` | LBK af lov om renter og andre forhold ved forsinket betaling | Justitsministeriet | §§ 1-9b | [retsinformation](https://www.retsinformation.dk/eli/lta/2014/459) | [xml](https://www.retsinformation.dk/eli/lta/2014/459/xml) |
| `DK-UDENRETLIGE-INDDRIVELSESOMKOSTNINGER-AMEND-2013-105` | BEK om ændring af bek om udenretlige inddrivelsesomkostninger | Justitsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2013/105) | [xml](https://www.retsinformation.dk/eli/lta/2013/105/xml) |
| `DK-AARSREGNSKABSLOVEN-2024-1140` | LBK af årsregnskabsloven | Erhvervsministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2024/1140) | [xml](https://www.retsinformation.dk/eli/lta/2024/1140/xml) |
| `EU-GDPR-2016-679` | Regulation (EU) 2016/679 (GDPR) | Europa-Parlamentet og Rådet | art. 15, 17 | [eur-lex](https://eur-lex.europa.eu/eli/reg/2016/679/oj) | — |
| `DK-OFFENTLIGE-BETALINGER-2007-798` | LBK af lov om offentlige betalinger m.v. | Finansministeriet | hele | [retsinformation](https://www.retsinformation.dk/eli/lta/2007/798) | [xml](https://www.retsinformation.dk/eli/lta/2007/798/xml) |

Den maskinlæsbare form af tabellen ligger i
[`sources/legal-sources.json`](../../sources/legal-sources.json), og
in-scope-rangerne i [`sources/scope.yaml`](../../sources/scope.yaml). De
to filer fødes ind i compliance-rapporteringen via
[`src/core/regulatory-coverage.ts`](../../src/core/regulatory-coverage.ts).

---

## 3. Matricen pr. lovkilde

### 3.1 Bogføringsloven (LOV 700/2022)

[ELI](https://www.retsinformation.dk/eli/lta/2022/700) · [XML](https://www.retsinformation.dk/eli/lta/2022/700/xml) · in_scope: §§ 1-18

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-BOOKKEEPING-BALANCED-001](../../rules/dk/bookkeeping.yaml#L7) | (§§ 7-9 generelt) | Dobbelt-bogføring: hver postering balancerer debet/kredit. | [`src/core/ledger.ts:88`](../../src/core/ledger.ts#L88) | kode | — |
| [DK-BOOKKEEPING-APPEND-ONLY-001](../../rules/dk/bookkeeping.yaml#L15) | § 9, stk. 3; § 13, stk. 1 | Bogførte journalposteringer er append-only — ingen UPDATE eller DELETE. | [`src/core/ledger.ts:89`](../../src/core/ledger.ts#L89) | kode | — |
| [DK-BOOKKEEPING-REVERSAL-001](../../rules/dk/bookkeeping.yaml#L29) | § 9, stk. 3 | Rettelser sker via en enkelt sporbar modposteringspostering, ikke ved ændring af originalen. | [`src/core/ledger.ts:90`](../../src/core/ledger.ts#L90) | kode | — |
| [DK-BOOKKEEPING-DOCUMENT-001](../../rules/dk/bookkeeping.yaml#L45) | § 9, stk. 1; § 9, stk. 2 | Bogføringsposteringer skal have et bilag som dokumentation, hvor det er relevant. | [`src/core/ledger.ts:91`](../../src/core/ledger.ts#L91) | kode | — |
| [DK-BOOKKEEPING-BANK-IMPORT-001](../../rules/dk/bookkeeping.yaml#L58) | § 7, stk. 1; § 9, stk. 2 | Importerede banktransaktioner skal bevare dato, beløb, tekst og sporbar batch. | [`src/core/bank.ts:178`](../../src/core/bank.ts#L178) | kode | — |
| [DK-BANK-BALANCE-CONTINUITY-001](../../rules/dk/bank.yaml#L5) | § 11, stk. 1 | Importerede kontoudtog tjekkes for kontinuitet i løbende saldo (warning, ikke hard_stop). | [`src/core/bank.ts:181`](../../src/core/bank.ts#L181) | kode | — |
| [DK-BOOKKEEPING-RECONCILIATION-001](../../rules/dk/bookkeeping.yaml#L79) | § 11, stk. 1; § 11, stk. 2 | Bank-afstemning skal vise matchede og umatchede transaktioner i en periode. | [`src/core/reconciliation.ts:65`](../../src/core/reconciliation.ts#L65) | kode | — |
| [DK-DOCUMENT-INTEGRITY-001](../../rules/dk/documents.yaml#L72) | § 9, stk. 3; § 13, stk. 1 | Originale bilag bevares uden silent overwrite — sha256 content-addressed, append-only audit. | [`src/core/documents.ts:53`](../../src/core/documents.ts#L53) | kode | — |
| [DK-BOOKKEEPING-RETENTION-001](../../rules/dk/bookkeeping.yaml#L238) | § 12, stk. 1 | Regnskabsmateriale skal kunne identificeres med `retain_until` = regnskabsårets udgang + 5 år. | [`src/core/retention.ts:23`](../../src/core/retention.ts#L23), [`src/core/gdpr.ts:39`](../../src/core/gdpr.ts#L39) | kode | — |
| [DK-ASSET-DEPR-001](../../rules/dk/bookkeeping.yaml#L256) | (§§ 7, 9 generelt) | Anlægsaktiv-afskrivninger skal posteres som deterministiske, balancerede periode-entries linket til købet. | [`src/core/assets.ts:27`](../../src/core/assets.ts#L27) | kode | — |
| [DK-ASSET-WRITEOFF-001](../../rules/dk/bookkeeping.yaml#L269) | (§§ 7, 9 generelt) | Straks-afskrivning af småanskaffelser kræver eksplicit bekræftelse + kildebakket tærskel-regel. | [`src/core/assets.ts:28`](../../src/core/assets.ts#L28) | hybrid | — |
| [DK-BOOKKEEPING-ACCRUAL-001](../../rules/dk/bookkeeping.yaml#L285) | (§§ 7, 9 generelt) | Periodeafgrænsninger: balanceret park-postering + deterministiske balancerede periode-entries der summer eksakt. | [`src/core/accruals.ts:44`](../../src/core/accruals.ts#L44) | kode | — |
| [DK-PAYABLE-001](../../rules/dk/bookkeeping.yaml#L301) | (§§ 7, 9 generelt) | En registreret leverandørfaktura er en sporbar, balanceret åben kreditorpost. | [`src/core/payables.ts:29`](../../src/core/payables.ts#L29) | kode | — |
| [DK-PAYABLE-PAYMENT-001](../../rules/dk/bookkeeping.yaml#L316) | (§§ 7, 9 generelt) | Kreditor-betalinger skal anvendes sporbart mod åbne kreditorposter og banktransaktioner. | [`src/core/payables.ts:30`](../../src/core/payables.ts#L30) | kode | — |
| [DK-EMAIL-DELIVERY-001](../../rules/dk/email.yaml#L5) | (§ 9 generelt) | Email-afsendelse af faktura/rykker er deterministisk, idempotent, og logges append-only. | [`src/core/email.ts:24`](../../src/core/email.ts#L24) | kode | — |
| [DK-MAIL-INTAKE-DEDUP-001](../../rules/dk/mail-intake.yaml#L17) | (§§ 7, 9 generelt) | Mail-indtagelse deduplikerer på stabil message-id + attachment-hash. | [`src/core/mail-intake.ts:30`](../../src/core/mail-intake.ts#L30) | kode | — |
| [DK-MAIL-INTAKE-EXCEPTION-001](../../rules/dk/mail-intake.yaml#L28) | § 1, stk. 1 (1383) | Beskeder uden brugbar vedhæftning eller med tvetydige metadata routes til exception-køen i stedet for at gætte. | [`src/core/mail-intake.ts:31`](../../src/core/mail-intake.ts#L31) | menneske | — |
| [DK-MILEAGE-LOG-001](../../rules/dk/mileage.yaml#L5) | (§ 9 generelt) | En kørebogspost er en komplet, kilde-bakket record før den gemmes. | [`src/core/mileage.ts:24`](../../src/core/mileage.ts#L24) | kode | — |
| [DK-INVOICE-LATE-INTEREST-BOOKKEEPING-001](../../rules/dk/invoices.yaml#L280) | § 9, stk. 1 | Registrerede morarente-krav skal bogføres én gang til tilgodehavender + ikke-moms krav-indtægt. | [`src/core/invoice-interest.ts:10`](../../src/core/invoice-interest.ts#L10) | kode | — |
| [DK-INVOICE-REMINDER-FEE-BOOKKEEPING-001](../../rules/dk/invoices.yaml#L318) | § 9, stk. 1 | Registrerede rykkergebyrer skal bogføres én gang til tilgodehavender + ikke-moms krav-indtægt. | [`src/core/invoice-reminders.ts:9`](../../src/core/invoice-reminders.ts#L9) | kode | — |
| [DK-INVOICE-LATE-COMPENSATION-BOOKKEEPING-001](../../rules/dk/invoices.yaml#L363) | § 9, stk. 1 | Registrerede DKK 310-kompensationskrav skal bogføres én gang til tilgodehavender + ikke-moms krav-indtægt. | [`src/core/invoice-compensation.ts:10`](../../src/core/invoice-compensation.ts#L10) | kode | — |
| [DK-INVOICE-CLAIM-SETTLEMENT-001](../../rules/dk/invoices.yaml#L432) | (§ 9 generelt) | Bank-modtagne krav-indbetalinger skal udligne bogførte krav-tilgodehavender. | [`src/core/invoice-claim-settlement.ts:7`](../../src/core/invoice-claim-settlement.ts#L7) | kode | — |
| [DK-INVOICE-COMBINED-SETTLEMENT-001](../../rules/dk/invoices.yaml#L450) | (§ 9 generelt) | Én indgående bank-modtagelse kan udligne både hovedstol og bogførte krav i én deterministisk anvendelse. | [`src/core/invoice-settlement.ts:8`](../../src/core/invoice-settlement.ts#L8) | kode | — |
| [DK-INVOICE-BAD-DEBT-WRITEOFF-001](../../rules/dk/invoices.yaml#L531) | § 9, stk. 1; § 9, stk. 3 | Uerholdelige kunde-tilgodehavender afskrives via append-only, faktura-linket korrektion. | [`src/core/invoice-bad-debt.ts:8`](../../src/core/invoice-bad-debt.ts#L8) | kode | — |

### 3.2 BEK 205/2024 — Ikke-registrerede digitale bogføringssystemer

[ELI](https://www.retsinformation.dk/eli/lta/2024/205) · [XML](https://www.retsinformation.dk/eli/lta/2024/205/xml) · in_scope: hele

Det er denne bekendtgørelse Rentemester operere efter, fordi systemet
ikke er registreret hos Erhvervsstyrelsen som et kommercielt digitalt
bogføringssystem (jf. bogføringslovens § 15-16).

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-BOOKKEEPING-FX-001](../../rules/dk/bookkeeping.yaml#L97) | § 3, stk. 1, nr. 5 | Ikke-DKK-transaktioner skal persistere valutakode, vekselfaktor pr. transaktionsdag, og DKK-konverteret beløb. | [`src/core/bank.ts:179`](../../src/core/bank.ts#L179), [`src/core/ledger.ts:92`](../../src/core/ledger.ts#L92) | kode | — |
| [DK-BOOKKEEPING-BACKUP-001](../../rules/dk/bookkeeping.yaml#L113) | § 4, stk. 1 | Mindst ugentlig fuld backup af bogførte transaktioner + bilag, med manifest med timestamp og hashes. | [`src/core/system-backups.ts:10`](../../src/core/system-backups.ts#L10), [`src/core/backup-governance.ts:32`](../../src/core/backup-governance.ts#L32) | hybrid | [backup-destinations.md](backup-destinations.md) |
| [DK-BOOKKEEPING-BACKUP-DEST-001](../../rules/dk/bookkeeping.yaml#L130) | § 4, stk. 2; § 4, stk. 3 | Backup opbevares hos ikke-nærtstående part der formodes at opfylde anerkendte it-sikkerhedsstandarder, på en server i et EU/EØS-land. Håndhæves som menneske-signeret attestering pr. destination. | [`backup-governance.ts:186`](../../src/core/backup-governance.ts#L186) (`isCompliantDestination`) | **menneske** | [Google Workspace-guide](backup-destinations/google-workspace.md) · [TEMPLATE](backup-destinations/_TEMPLATE.md) |
| [DK-BOOKKEEPING-BACKUP-KEY-ROTATE-001](../../rules/dk/bookkeeping.yaml#L150) | § 4, stk. 1 | Rotation af Ed25519 backup-signaturnøgle skal være auditerbar og bevare verifikation af ældre backups. | [`src/core/system-backups.ts:566`](../../src/core/system-backups.ts#L566) | menneske | [backup-security.md](../backup-security.md) |
| [DK-BOOKKEEPING-PERIOD-LOCK-001](../../rules/dk/bookkeeping.yaml#L222) | § 3, stk. 3 | Posteringer må ikke ramme lukkede perioder, og må ikke være fremtidsdaterede. | [`src/core/periods.ts:219`](../../src/core/periods.ts#L219), [`src/core/ledger.ts:93`](../../src/core/ledger.ts#L93) | kode | — |

### 3.3 BEK 97/2023 — Digitale standardbogføringssystemer (benchmark)

[ELI](https://www.retsinformation.dk/eli/lta/2023/97) · [XML](https://www.retsinformation.dk/eli/lta/2023/97/xml) · in_scope: hele

Rentemester er ikke registreret som standard-system, men opfylder
benchmark-krav til export og restore, så det er klar til registrering
hvis det engang bliver krævet.

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-BOOKKEEPING-AUTHORITY-EXPORT-001](../../rules/dk/bookkeeping.yaml#L167) | § 11, stk. 1; § 11, stk. 4 | Bogføringsdata + bilag eksporteres pr. periode i struktureret maskinlæsbar pakke til myndighed. | [`src/core/authority-export.ts:10`](../../src/core/authority-export.ts#L10) | kode | — |
| [DK-BOOKKEEPING-SAFT-EXPORT-001](../../rules/dk/bookkeeping.yaml#L186) | § 11, stk. 4 | SAF-T export skal være deterministisk, periode-bundet, med manifest-hashes og eksplicitte out-of-scope-sektioner. | [`src/core/saft-export.ts:103`](../../src/core/saft-export.ts#L103) | kode | — |
| [DK-BOOKKEEPING-RESTORE-001](../../rules/dk/bookkeeping.yaml#L205) | § 10, stk. 1 | Backup-materiale skal kunne restores til et læsbart selskabs-dataset, med manifest- + hash-verifikation først. | [`src/core/system-restore.ts:13`](../../src/core/system-restore.ts#L13) | kode | — |

### 3.4 BEK 1383/2023 — Pligt til opbevaring af bilag

[ELI](https://www.retsinformation.dk/eli/lta/2023/1383) · [XML](https://www.retsinformation.dk/eli/lta/2023/1383/xml) · in_scope: hele · ændret af [BEK 302/2025](https://www.retsinformation.dk/eli/lta/2025/302) fra 2026-01-01.

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-DOCUMENT-STORAGE-001](../../rules/dk/documents.yaml#L7) | § 1, stk. 1, nr. 1-6 | Digitalt opbevarede købs/salgsbilag skal indeholde 7 statutoriske minimumsfelter (afsender, modtager, dato, beløb, moms, betalingsdetaljer, leveringsbeskrivelse). | [`src/core/documents.ts:50`](../../src/core/documents.ts#L50) | kode | — |
| [DK-DOCUMENT-CASH-RECEIPT-001](../../rules/dk/documents.yaml#L40) | § 1, stk. 2 | Kassestrimler kan opbevares uden fuld minimumssæt, men original-fil + valutakode bevares. | [`src/core/documents.ts:51`](../../src/core/documents.ts#L51) | kode | — |
| [DK-DOCUMENT-FOREIGN-PHYSICAL-001](../../rules/dk/documents.yaml#L56) | § 1, stk. 3 | Fremmedlandske fysiske-bilag kan opbevares uden dansk minimumssæt, men original + valuta bevares. | [`src/core/documents.ts:52`](../../src/core/documents.ts#L52) | kode | — |
| [DK-MASTER-DATA-VENDOR-001](../../rules/dk/documents.yaml#L88) | § 1, stk. 1; § 1, stk. 1, nr. 4 | Leverandør-stamdata genbrugt på bilag skal opløses deterministisk og materialiseres ind i bilags-metadata ved indtagelse. | [`src/core/master-data.ts:173`](../../src/core/master-data.ts#L173) | kode | — |
| [DK-MAIL-INTAKE-TRANSPORT-001](../../rules/dk/mail-intake.yaml#L6) | (advisory) | Første understøttede mail-transport er en lokal maildrop eller EML, ikke en hostet mailbox. | [`src/core/mail-intake.ts:29`](../../src/core/mail-intake.ts#L29) | kode | — |

### 3.5 Momsloven (LBK 209/2024)

[ELI](https://www.retsinformation.dk/eli/lta/2024/209) · [XML](https://www.retsinformation.dk/eli/lta/2024/209/xml) · in_scope: §§ 23, 27, 37-42, 46, 47-57

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-INVOICE-DELIVERY-DATE-001](../../rules/dk/invoices.yaml#L111) | § 23, stk. 2 | Faktura skal angive leveringsdato når den afviger fra udstedelsesdato. | [`src/core/invoice.ts:62`](../../src/core/invoice.ts#L62) | kode | — |
| [DK-VAT-REPORT-001](../../rules/dk/vat.yaml#L56) | § 56, stk. 1-3 | Moms-afregningsgrundlag skal vise output-moms, input-moms og netto-skyld for en periode. | [`src/core/vat.ts:54`](../../src/core/vat.ts#L54) | kode | — |
| [DK-VAT-REVERSE-CHARGE-001](../../rules/dk/vat.yaml#L77) | § 46, stk. 1, nr. 3 | EU-ydelseskøb med omvendt betalingspligt skal bogføre både output- og fradragsberettiget input-moms i perioden. | [`src/core/vat.ts:55`](../../src/core/vat.ts#L55), [`src/core/vies.ts:26`](../../src/core/vies.ts#L26) | kode | — |
| [DK-VAT-REPRESENTATION-001](../../rules/dk/vat.yaml#L93) | § 42, stk. 1, nr. 5; § 42, stk. 2 | Repræsentationsindkøb må kun bogføre 25 % af momsen som fradragsberettiget input. | [`src/core/vat.ts:56`](../../src/core/vat.ts#L56) | kode | — |
| [DK-VAT-BAD-DEBT-001](../../rules/dk/vat.yaml#L112) | § 27, stk. 6 | Konstaterede uerholdelige tab på standard-momsalg skal reducere output-moms ud fra 80 %-tabsgrundlaget. | [`src/core/invoice-bad-debt.ts:9`](../../src/core/invoice-bad-debt.ts#L9) | kode | — |
| [DK-VAT-FILING-001](../../rules/dk/vat.yaml#L131) | § 57, stk. 1 | Indberetningsklar momsangivelse må kun produceres for en lukket momsperiode og skal mappe til SKAT-rubrikker. | [`src/core/vat-filing.ts:59`](../../src/core/vat-filing.ts#L59) | kode | — |
| [DK-VAT-EU-SALES-LIST-001](../../rules/dk/vat.yaml#L153) | § 54, stk. 1 | EU B2B-salg uden dansk moms grupperes pr. kunde-CVR/VAT for EU-salg-uden-moms-listen. | [`src/core/vat-vies-list.ts:26`](../../src/core/vat-vies-list.ts#L26) | kode | — |
| [DK-VAT-OSS-001](../../rules/dk/vat.yaml#L170) | (§§ 66-66m via momsloven OSS-kap.) | OSS-salg til EU-forbrugere holdes ude af standard-momsangivelsen. | [`src/core/vat-oss.ts:32`](../../src/core/vat-oss.ts#L32) | kode | — |

### 3.6 Momsbekendtgørelsen (BEK 1435/2023)

[ELI](https://www.retsinformation.dk/eli/lta/2023/1435) · [XML](https://www.retsinformation.dk/eli/lta/2023/1435/xml) · in_scope: §§ 58-72

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-INVOICE-FULL-001](../../rules/dk/invoices.yaml#L8) | § 58, stk. 1, nr. 1-9 | Fuld faktura skal indeholde 9 minimumsfelter (sælger, køber, dato, nr., beskrivelse, beløb, moms-sats, moms-beløb, ydelses-/leveringstidspunkt). | [`src/core/invoice.ts:58`](../../src/core/invoice.ts#L58) | kode | — |
| [DK-INVOICE-SIMPLIFIED-001](../../rules/dk/invoices.yaml#L45) | § 66, stk. 1, nr. 1-5 | Forenklet faktura er kun tilladt op til DKK 3.000 brutto, med reduceret minimumssæt. | [`src/core/invoice.ts:59`](../../src/core/invoice.ts#L59) | kode | — |
| [DK-INVOICE-REVERSE-CHARGE-001](../../rules/dk/invoices.yaml#L75) | § 59, stk. 1; § 60, stk. 1 | Reverse-charge fakturaer skal undlade moms-beløb/-sats og bære reverse-charge-note. | [`src/core/invoice.ts:60`](../../src/core/invoice.ts#L60) | kode | — |
| [DK-INVOICE-REVERSE-CHARGE-BASIS-001](../../rules/dk/invoices.yaml#L93) | § 60, stk. 1; § 62, stk. 1 | Reverse-charge faktura skal angive den specifikke lovbestemmelse som fritagelsesgrundlag. | [`src/core/invoice.ts:61`](../../src/core/invoice.ts#L61) | kode | — |
| [DK-INVOICE-ARITHMETIC-001](../../rules/dk/invoices.yaml#L127) | § 58, stk. 1, nr. 7, 9 | Faktura-linjesummer og totaler skal stemme aritmetisk før udstedelse. | [`src/core/invoice.ts:63`](../../src/core/invoice.ts#L63) | kode | — |
| [DK-INVOICE-ISSUE-001](../../rules/dk/invoices.yaml#L146) | § 58, stk. 1, nr. 2 | Udstedte fakturaer skal lagres immutabelt med sekventielle fakturanumre. | [`src/core/invoice-pdf.ts:11`](../../src/core/invoice-pdf.ts#L11), [`src/core/issued-invoices.ts:39`](../../src/core/issued-invoices.ts#L39) | kode | — |
| [DK-MASTER-DATA-CUSTOMER-001](../../rules/dk/invoices.yaml#L162) | § 58, stk. 1; § 58, stk. 1, nr. 4 | Kunde-stamdata genbrugt på fakturaer skal opløses deterministisk før udstedelse. | [`src/core/master-data.ts:107`](../../src/core/master-data.ts#L107) | kode | — |
| [DK-INVOICE-BOOKKEEPING-001](../../rules/dk/invoices.yaml#L380) | § 58, stk. 1, nr. 7, 9 | Udstedte salgsfakturaer skaber én sporbar tilgodehavende-, omsætnings- og moms-journalpostering. | [`src/core/invoice-booking.ts:5`](../../src/core/invoice-booking.ts#L5) | kode | — |
| [DK-INVOICE-BOOKKEEPING-REVERSE-002](../../rules/dk/invoices.yaml#L399) | § 59, stk. 1; § 60, stk. 1 | Reverse-charge salgsfakturaer og fallback-kreditnotaer posteres uden output-moms-linje. | [`src/core/invoice-booking.ts:6`](../../src/core/invoice-booking.ts#L6), [`src/core/credit-notes.ts:38`](../../src/core/credit-notes.ts#L38) | kode | — |
| [DK-INVOICE-SETTLEMENT-001](../../rules/dk/invoices.yaml#L417) | (§ 58 kvitterings-spor) | Bank-modtagne kunde-betalinger udligner tilgodehavender med sporbare faktura- og bank-links. | [`src/core/invoice-settlement.ts:7`](../../src/core/invoice-settlement.ts#L7) | kode | — |
| [DK-INVOICE-PAYMENT-001](../../rules/dk/invoices.yaml#L467) | (§ 58 kvitterings-spor) | Faktura-betalinger skal anvendes sporbart mod udstedte fakturaer og banktransaktioner. | [`src/core/invoice-payments.ts:115`](../../src/core/invoice-payments.ts#L115) | kode | — |
| [DK-INVOICE-CORRECTION-BALANCE-001](../../rules/dk/invoices.yaml#L481) | (§ 58 kvitterings-spor) | Åben faktura-saldo skal afspejle linkede kreditnotaer såvel som betalinger. | [`src/core/invoice-payments.ts:116`](../../src/core/invoice-payments.ts#L116) | kode | — |
| [DK-CREDIT-NOTE-001](../../rules/dk/invoices.yaml#L494) | § 58, stk. 2; § 66, stk. 1, nr. 6 | Kreditnotaer skal referere den oprindelige udstedte faktura og spejle den korrigerede moms-effekt. | [`src/core/credit-notes.ts:37`](../../src/core/credit-notes.ts#L37) | kode | — |
| [DK-INVOICE-REFUND-001](../../rules/dk/invoices.yaml#L515) | (§ 58 kvitterings-spor) | Kunde-refunderinger efter kreditnotaer udlignes sporbart mod bank og korrigeret faktura-saldo. | [`src/core/invoice-refunds.ts:7`](../../src/core/invoice-refunds.ts#L7) | kode | — |
| [DK-INVOICE-LOCK-001](../../rules/dk/invoices.yaml#L551) | § 58, stk. 2 | Udstedte fakturaer kan ikke ændres. | [`src/core/issued-invoices.ts:40`](../../src/core/issued-invoices.ts#L40) | kode | — |
| [DK-RECURRING-INVOICE-TEMPLATE-001](../../rules/dk/invoices.yaml#L564) | (§ 58 forhåndsvalidering) | Gentagende-faktura-skabeloner gemmer en immutabel, præ-valideret faktura-specifikation. | [`src/core/recurring-invoices.ts:81`](../../src/core/recurring-invoices.ts#L81) | kode | — |
| [DK-VAT-SEPARATE-AMOUNT-001](../../rules/dk/vat.yaml#L42) | § 58, stk. 1, nr. 8, 9 | Dansk moms-fradrag kræver angivet moms-beløb. | [`src/core/invoice.ts:64`](../../src/core/invoice.ts#L64) | kode | — |

### 3.7 Renteloven (LBK 459/2014)

[ELI](https://www.retsinformation.dk/eli/lta/2014/459) · [XML](https://www.retsinformation.dk/eli/lta/2014/459/xml) · in_scope: §§ 1-9b

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-INVOICE-DUE-DATE-001](../../rules/dk/invoices.yaml#L229) | § 3, stk. 1; § 3, stk. 2 | Kunde-fakturaer skal eksponere deterministisk forfaldsdato og overdue-klassifikation. | [`src/core/invoice-payments.ts:117`](../../src/core/invoice-payments.ts#L117) | kode | — |
| [DK-INVOICE-LATE-INTEREST-001](../../rules/dk/invoices.yaml#L246) | § 5, stk. 1 | Overdue kunde-fakturaer skal understøtte deterministisk statutorisk morarenteberegning. | [`src/core/invoice-interest.ts:8`](../../src/core/invoice-interest.ts#L8) | kode | — |
| [DK-INVOICE-LATE-INTEREST-REGISTER-001](../../rules/dk/invoices.yaml#L263) | § 5, stk. 1 | Et morarente-krav må kun registreres fra deterministisk beregning og forblive sporbart i krav-saldoen. | [`src/core/invoice-interest.ts:9`](../../src/core/invoice-interest.ts#L9) | kode | — |
| [DK-INVOICE-REMINDER-FEE-001](../../rules/dk/invoices.yaml#L297) | § 9b, stk. 1, 2 | Rykkergebyrer skal respektere statutorisk maxbeløb, max-antal og spacing (10 dage). | [`src/core/invoice-reminders.ts:8`](../../src/core/invoice-reminders.ts#L8) | kode | — |

### 3.8 BEK 105/2013 — Udenretlige inddrivelsesomkostninger (kompensation)

[ELI](https://www.retsinformation.dk/eli/lta/2013/105) · [XML](https://www.retsinformation.dk/eli/lta/2013/105/xml) · in_scope: hele

Indfører den faste DKK 310-kompensation for forsinket erhvervsmæssig betaling fra 2013-03-01.

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-INVOICE-LATE-COMPENSATION-001](../../rules/dk/invoices.yaml#L335) | (hele bek.) | Overdue erhvervs-fakturaer skal understøtte deterministisk vurdering af det faste kompensationskrav. | [`src/core/invoice-compensation.ts:8`](../../src/core/invoice-compensation.ts#L8) | kode | — |
| [DK-INVOICE-LATE-COMPENSATION-REGISTER-001](../../rules/dk/invoices.yaml#L348) | (hele bek.) | Et fast kompensationskrav må kun registreres én gang pr. overdue erhvervsfaktura og forblive sporbart. | [`src/core/invoice-compensation.ts:9`](../../src/core/invoice-compensation.ts#L9) | kode | — |

### 3.9 Årsregnskabsloven (LBK 1140/2024)

[ELI](https://www.retsinformation.dk/eli/lta/2024/1140) · [XML](https://www.retsinformation.dk/eli/lta/2024/1140/xml) · in_scope: hele

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-ANNUAL-REPORT-CLASS-B-001](../../rules/dk/annual-report.yaml#L50) | (regnskabsklasse B) | En årsrapport kan kun samles for et fuldt låst regnskabsår med komplette company master data. | [`src/core/annual-report.ts:28`](../../src/core/annual-report.ts#L28) | kode | — |
| [DK-ANNUAL-REPORT-IXBRL-002](../../rules/dk/annual-report.yaml#L69) | (digital indberetning) | iXBRL-output skal være deterministisk og begrænset til den deklarerede micro/small-taksonomi-subset. | [`src/core/ixbrl.ts:30`](../../src/core/ixbrl.ts#L30) | kode | — |
| [DK-TAX-RETURN-CORP-001](../../rules/dk/tax-return.yaml#L18) | (skattepligtig indkomst) | Selskabsskattepligtig indkomst forberedes deterministisk fra låst årsrapport; ikke-deterministiske justeringer surfaces som needs-review. | [`src/core/tax-return.ts:36`](../../src/core/tax-return.ts#L36) | advisory | — |

### 3.10 GDPR (forordning 2016/679)

[EUR-Lex](https://eur-lex.europa.eu/eli/reg/2016/679/oj) · in_scope: art. 15, 17

GDPR er direkte gældende i Danmark. Rentemester implementerer to flows:
indsigt (art. 15) og sletning (art. 17). Art. 17, stk. 3, lit. b og e
giver det lovgrundlag Rentemester læner sig op af, når sletning af
regnskabsmateriale afvises inden for bogføringspligtens retention-vindue.

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [GDPR-SUBJECT-EXPORT](../../rules/dk/gdpr.yaml#L5) | art. 15 | Et registreret subjekt skal kunne få en struktureret kopi af sine persondata i bogføringssystemet. Tidligere redigerede felter forbliver redigeret. | [`src/core/gdpr.ts:37`](../../src/core/gdpr.ts#L37) | kode | — |
| [GDPR-RETENTION-BOUNDED-ERASURE](../../rules/dk/gdpr.yaml#L17) | art. 17, stk. 3, lit. b, e | Sletningsanmodninger respekterer bogføringspligtens retention-deadline; afvisning logges som append-only tombstone i `gdpr_erasures`. | [`src/core/gdpr.ts:38`](../../src/core/gdpr.ts#L38) | kode | — |

Begge regler kobler tæt til [DK-BOOKKEEPING-RETENTION-001](#31-bogføringsloven-lov-7002022)
— retention-deadlinen er det signal, der gør sletning lovlig (eller
ulovlig på et givet tidspunkt).

### 3.11 Lov om offentlige betalinger (LBK 798/2007)

[ELI](https://www.retsinformation.dk/eli/lta/2007/798) · [XML](https://www.retsinformation.dk/eli/lta/2007/798/xml) · in_scope: hele

Loven der pålægger offentlige myndigheder at modtage elektroniske
fakturaer. De fire NemHandel-/PEPPOL-regler hørte tidligere fejlagtigt
under momsbekendtgørelsen, men deres juridiske grundlag er her.

| Rule | Paragraf | Hvad kræves | Håndhævelse | Karakter | Guide |
|---|---|---|---|---|---|
| [DK-INVOICE-PUBLIC-RECIPIENT-001](../../rules/dk/invoices.yaml#L179) | (offentlig-modtager-pligt) | Fakturaer til offentlige modtagere skal bære gyldig EAN/GLN i immutabel buyer-snapshot. | [`src/core/invoice.ts:65`](../../src/core/invoice.ts#L65) | kode | — |
| [DK-INVOICE-PUBLIC-EXPORT-001](../../rules/dk/invoices.yaml#L193) | (offentlig-modtager-pligt) | Offentlig-fakturas eksport-preview skal være deterministisk og transport-fri handoff. | [`src/core/public-einvoice.ts:9`](../../src/core/public-einvoice.ts#L9) | kode | — |
| [DK-INVOICE-PUBLIC-OIOUBL-001](../../rules/dk/invoices.yaml#L208) | (offentlig-modtager-pligt) | Offentlig-fakturas OIOUBL-handoff-export er deterministisk, auditerbar, transport-bundet. | [`src/core/public-einvoice.ts:10`](../../src/core/public-einvoice.ts#L10) | kode | — |
| [DK-PEPPOL-SUBMIT-001](../../rules/dk/peppol.yaml#L5) | (offentlig-modtager-pligt) | PEPPOL-indsendelse er deterministisk, idempotent, baseret på OIOUBL-handoff-artefaktet. | [`src/core/public-einvoice.ts:25`](../../src/core/public-einvoice.ts#L25) | hybrid | [peppol-nemhandel.md](../peppol-nemhandel.md) |

> **Note:** Lovgrundlaget mandater pligten til at sende elektronisk;
> de tekniske formater (OIOUBL, PEPPOL BIS Billing 3) er specificeret i
> underliggende bekendtgørelser. Den tekniske implementering er dækket
> separat i [docs/peppol-nemhandel.md](../peppol-nemhandel.md).

---

## 4. Interne (ikke-lovbestemte) regler

Nogle rule_ids er Rentemester-specifikke kvalitets-/integritets-regler
uden direkte lovkilde. De følger samme håndhævelsesmønster, men kan
flyttes/omformuleres uden at det rører dansk ret.

| Rule | Hvad det dækker | Håndhævelse |
|---|---|---|
| `DK-RUNTIME-AGENT-001` | Runtime-agent contract: hvilke MCP-tools en agent må kalde og under hvilke betingelser. | [`src/agent/contract.ts:21`](../../src/agent/contract.ts#L21) |
| `DK-IMPORT-POSTINGS-001` | Import af Dinero-postninger ved migration ind i Rentemester. | [`src/core/import/dinero-postings.ts:50`](../../src/core/import/dinero-postings.ts#L50) |
| `DK-IMPORT-CHART-RECONCILE-001` | Kontoplan-rekonciliering ved import. | [`src/core/import/reconcile.ts:33`](../../src/core/import/reconcile.ts#L33) |
| `DK-IMPORT-COMPANY-RECONCILE-001` | Virksomhedsmetadata-rekonciliering ved import. | [`src/core/import/reconcile.ts:34`](../../src/core/import/reconcile.ts#L34) |
| `DK-RECURRING-INVOICE-GENERATE-001` | Genereringsskridtet for gentagende fakturaer. | [`src/core/recurring-invoices.ts:82`](../../src/core/recurring-invoices.ts#L82) |

---

## 5. Sådan holder du matricen aktuel

Når der lægges en ny regel til:

1. Tilføj rule_id'et i den relevante `rules/dk/*.yaml` med `source_id`,
   `provisions[]` (med `ref:` + `text_hash:`), `severity` og `machine_rule`.
2. Implementer håndhævelsen i `src/core/<modul>.ts` og deklarer
   `const RULE_ID = "DK-…";` så grep nemt finder enforcement-punktet.
3. Tilføj en række i den relevante kildesektion herover.
4. Hvis reglen er **hybrid** eller **menneske**-attesteret, lav (eller
   link til) en guide under `docs/compliance/<emne>/<udbyder>.md`.
5. Hvis kilden er en ny lov/bekendtgørelse, opdatér
   [`sources/legal-sources.json`](../../sources/legal-sources.json) og
   evt. [`sources/scope.yaml`](../../sources/scope.yaml), og tilføj en
   sektion under [§ 3](#3-matricen-pr-lovkilde) i denne fil.

Når en regel ændrer paragraf-reference (fx ved en ny bekendtgørelse):

1. Opdatér `provisions[].ref` og `text_hash:` i YAML.
2. Opdatér paragraf-kolonnen i matricen.
3. Tjek at `xmlUrl` i `legal-sources.json` stadig peger på den gældende
   konsolidering.

Determinismetjek af hele matricen kan i fremtiden automatiseres ved at:

- grep'e `rules/dk/*.yaml` for alle rule_ids
- grep'e `src/` for samme rule_ids
- diff'e mod tabellerne i denne fil

Indtil videre vedligeholdes matricen manuelt — det er en lille corpus,
og en pull request på reglen er et naturligt sted at opdatere
dokumentationen i samme commit.
