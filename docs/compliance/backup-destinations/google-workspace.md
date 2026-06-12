---
udbyder: Google Workspace (Business Standard og opefter)
status: egnet
sidst-verificeret: 2026-05-24
verificeret-af: Mikkel Krogsholm
master-guide: ../backup-destinations.md
---

# Google Workspace med Data Regions = Europa

Konkret opskrift på hvordan en Google Workspace-konto opfylder
BEK 205/2024 § 4, stk. 2 som destination for den ugentlige backup.

## 1. Forudsætninger

| Krav | Hvad du skal have |
|---|---|
| Plan | **Google Workspace Business Standard** eller højere. Business Starter og almindelige Google One/forbrugerkonti har ikke Data Regions og er ikke egnede. |
| Admin-rolle | Super Admin eller en delegeret rolle med adgang til `Data → Compliance → Dataregioner` i admin.google.com. |
| Klient | Google Drive til desktop installeret på den maskine, hvor Rentemester kører — så `~/Library/CloudStorage/GoogleDrive-<konto>/` er synkroniseret. På Linux fungerer det via `rclone` mod den samme Workspace-konto. |
| Custom Domain | Workspace-kontoen skal være knyttet til et domæne (du logger ind med `<dig>@<domæne>.dk`, ikke `@gmail.com`). |

## 2. Slå Data Regions = Europa til

1. Log ind på <https://admin.google.com> som Super Admin.
2. Navigér til **Data → Overholdelse → Dataregioner → Region**.
3. Vælg organisationsenhed (OU) eller "Hele organisationen".
4. Under "Region for opbevaring af inaktive data" vælg **Europa**.
5. Klik **Gem**.

Det tager fra timer til dage før eksisterende data er migreret. Nye
backups vil lande i Europa næsten med det samme, men du bør **vente på
at admin-konsollen viser "fuldført" under "Statussen"-linket**, før du
attesterer i Rentemester.

### Hvad Data Regions dækker

- Primær data-at-rest i kerne-Workspace-tjenester (Gmail, Drive, Docs,
  Sheets, Slides, Calendar, Meet-optagelser, Tasks, Keep, Sites, Vault).
- For backup-formål er det Drive der tæller — dit backup-arkiv er en
  almindelig fil i Drive.

### Hvad Data Regions ikke dækker

- Indekser, midlertidige caches og enkelte metadata-felter kan stadig
  være globale. For en `.tar`-fil er det irrelevant; indholdet
  (regnskabsmaterialet) er bag dataresidensen.
- Tredjepartsapps, Marketplace-add-ons og ikke-kernetjenester (Maps,
  Earth osv.).

Reference:
[Workspace Data Regions](https://support.google.com/a/answer/7630496).

## 3. Vælg en Drive-mappe til backupen

Opret en dedikeret mappe i Drive, fx `Rentemester-backup/`, så
backup-arkiverne ikke blandes med almindelige dokumenter:

```
Min Drev
└── Rentemester-backup
    ├── 2026-W18.tar
    ├── 2026-W19.tar
    └── 2026-W20.tar
```

Den lokale sti til mappen er typisk:

- **macOS**: `/Users/<dig>/Library/CloudStorage/GoogleDrive-<email>/Mit Drev/Rentemester-backup`
- **Windows**: `G:\Mit Drev\Rentemester-backup` (eller hvad du har valgt som Drive-bogstav)
- **Linux + rclone**: en mountpoint som `/mnt/gdrive/Rentemester-backup`

## 4. Attestér destinationen i Rentemester

```bash
bun run rentemester system backup-add-destination \
  --label "Google Workspace Drive (EU)" \
  --kind google-drive \
  --location "/Users/<dig>/Library/CloudStorage/GoogleDrive-<email>/Mit Drev/Rentemester-backup" \
  --region-eu true \
  --region-country EU \
  --region-note "GWS Data Regions = Europa, attesteret i admin.google.com → Data → Compliance → Dataregioner" \
  --non-related true \
  --it-security true \
  --it-security-note "Google Workspace: ISO/IEC 27001, 27017, 27018, SOC 2 Type II" \
  --attested-by "<Dit fulde navn>"
```

Hvad de tre flag betyder her:

- `--region-eu true`: **kun sand hvis du har bekræftet at Data Regions
  er sat til Europa for den OU kontoen tilhører.** Hvis du ikke selv har
  set indstillingen i admin-konsollen, må flaget være `false`.
- `--region-country` kan stå som `EU`, hvis du ikke ved hvilket konkret
  EU-datacenter Google har valgt. De typiske er Belgien (BE), Holland (NL),
  Finland (FI) og Tyskland (DE). Workspace dokumenterer ikke det konkrete
  datacenter pr. tenant.
- `--it-security true`: lovligt at sætte, fordi Google Workspace
  offentligt er certificeret efter [ISO/IEC 27001, 27017, 27018](https://workspace.google.com/security/) og [SOC 2 Type II](https://cloud.google.com/security/compliance/soc-2). Formodningsreglen er klart opfyldt.

Verificér derefter:

```bash
bun run rentemester system backup-destinations
bun run rentemester system backup-governance
```

## 5. Brug destinationen til en faktisk backup

1. Lav arkivet:
   ```bash
   bun run rentemester system backup --archive
   ```
2. Kopiér output-`.tar`-filen til Drive-mappen (eller lad agenten gøre
   det via `placeBackupArchive`).
3. Vent på at Drive-klienten har synkroniseret op til Workspace. Du kan
   se synkroniseringsstatus i Drive-menubaren.
4. Bekræft placeringen i Rentemester:
   ```bash
   bun run rentemester system backup-place \
     --backup-id <id> \
     --destination-id <dest-id> \
     --actor-kind human
   ```

Placeringen logges som `verified` hvis Rentemester selv kan
re-læse `.tar`-filen i destinationsstien og verificere sha256 mod sit
eget manifest. Hvis filen er flyttet via en kanal Rentemester ikke kan
genlæse, registreres den som `declared`.

## 6. Kontroltjek (mindst én gang om året)

| Tjek | Hvor |
|---|---|
| Data Regions stadig = Europa | admin.google.com → Data → Compliance → Dataregioner. |
| Plan stadig Business Standard+ | admin.google.com → Fakturering. |
| Workspace-certificeringer stadig gyldige | <https://cloud.google.com/security/compliance/iso-27001> |
| Drive-mappens lokale sti uændret | `ls "<location>"` på maskinen. |
| Ingen offentligt kendt sikkerhedshændelse der afkræfter it-sikkerhedsformodningen | Google Workspace's [status-dashboard](https://www.google.com/appsstatus/) og pressedækning. |

Hvis et af punkterne falder ud, fjern destinationen med
`system backup-remove-destination --id <id>` og attestér en ny.

## 7. Hvad denne opsætning **ikke** dækker

- **GDPR / Schrems II**: Google er et amerikansk selskab. CLOUD Act kan
  i teorien tvinge Google til at udlevere data uanset hvor det fysisk
  ligger. Det er et GDPR-spørgsmål (databehandleraftale, SCC,
  transfer impact assessment) — ikke et § 4, stk. 2-spørgsmål. § 4
  spørger om hvor serveren står, ikke om jurisdiktionen.
- **Kryptering at rest af selve backup-arkivet**: Google krypterer
  data-at-rest, men hvis du vil have envelope-encryption ovenpå, skal du
  kryptere `.tar`-filen lokalt før upload (fx med `age` eller `gpg`).
  Det er ikke et lovkrav, men best practice.
- **Restore-tests**: er en separat disciplin. Se
  `DK-BOOKKEEPING-RESTORE-001` og `system backup-restore`-kommandoen.

## 8. Kilder

- [BEK nr. 205 af 04/03/2024](https://www.retsinformation.dk/eli/lta/2024/205) — § 4, stk. 2.
- [Bogføringsloven, LOV 700/2022](https://www.retsinformation.dk/eli/lta/2022/700) — § 12, § 15.
- [Google Workspace Data Regions (support.google.com)](https://support.google.com/a/answer/7630496).
- [Google Workspace certifikater og audits](https://workspace.google.com/security/).
- Rentemester-kildehenvisninger: [backup-guide.ts:193-217](../../../src/core/backup-guide.ts#L193), [backup-governance.ts:186-191](../../../src/core/backup-governance.ts#L186).
