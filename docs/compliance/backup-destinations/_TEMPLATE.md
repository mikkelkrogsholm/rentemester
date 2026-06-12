---
udbyder: <Udbydernavn — fx "Microsoft 365 (E3) med EU Data Boundary">
status: <egnet | ikke-egnet | betinget>
sidst-verificeret: <YYYY-MM-DD>
verificeret-af: <Dit navn>
master-guide: ../backup-destinations.md
---

# <Udbyder> som backup-destination

> Kopiér denne fil til `<udbyder-kebab-case>.md` og udfyld alle felter.
> Slet placeholder-teksten i citatblokke som denne. En guide skal kunne
> følges af en ny bruger uden at de behøver ringe til udbyderen.

## 1. Forudsætninger

| Krav | Hvad du skal have |
|---|---|
| Plan | <hvilken licens/plan er nødvendig — hvorfor> |
| Admin-rolle | <hvilken rolle der kan slå dataresidens til> |
| Klient | <hvordan filerne ender lokalt så Rentemester kan lægge backup-arkivet der> |
| Øvrigt | <fx domæne, SSO, MFA-krav, regions-tilvalg> |

## 2. Slå EU/EØS-dataresidens til

1. <Navigation-sti i admin-konsollen, eksakte menunavne>
2. <Felt der skal sættes>
3. <Bekræftelse / migrations-status>

### Hvad dataresidens-indstillingen dækker

- <Hvilke services dækker den — er det Drive/Files/Object Storage?>
- <Hvad gælder for backup-arkivet?>

### Hvad den ikke dækker

- <Caches, metadata, indekser>
- <Plan-gating>
- <Andre tjenester i samme konto>

Reference: <officiel udbyder-doc-URL>.

## 3. Lokal sti / klient-opsætning

Beskriv hvor backup-arkivet skal lægges lokalt, så det automatisk
synkroniseres op til udbyderen:

```
<eksempel på sti og mappestruktur>
```

Pr. OS:

- **macOS**: `<sti>`
- **Windows**: `<sti>`
- **Linux**: `<rclone/sshfs/anden mount-løsning>`

## 4. Attestér destinationen i Rentemester

```bash
bun run rentemester system backup-add-destination \
  --label "<menneskelæsbart navn>" \
  --kind <local-folder|dropbox|google-drive|ssh|other> \
  --location "<lokal sti>" \
  --region-eu true \
  --region-country <ISO-3166-1 alpha-2 — fx DE, IE, EU> \
  --region-note "<hvor du har attesten fra, fx admin-konsol-sti>" \
  --non-related true \
  --it-security true \
  --it-security-note "<certificeringer, fx ISO 27001, SOC 2>" \
  --attested-by "<Dit fulde navn>"
```

Begrundelse for hver attestering:

- `--region-eu`: <link til kontrakt/dokumentation der binder dataresidens>
- `--non-related`: <hvorfor udbyderen er en uafhængig tredjepart>
- `--it-security`: <link til certificeringssider>

## 5. Brug destinationen

1. `bun run rentemester system backup --archive`
2. <Hvordan arkivet kommer over til udbyderen — manuel kopi, klient-sync, agent-push>
3. `bun run rentemester system backup-place --backup-id <id> --destination-id <dest-id> --actor-kind <human|agent>`

## 6. Kontroltjek

| Tjek | Hvor |
|---|---|
| Dataresidens stadig EU/EØS | <admin-konsol-sti> |
| Plan / licens uændret | <admin-konsol-sti> |
| Certificeringer stadig gyldige | <link til certificeringsside> |
| Lokal sti uændret | `ls "<location>"` |

## 7. Hvad opsætningen ikke dækker

- GDPR / jurisdiktion: <udbyderens HQ-land, transfer impact assessment-behov>
- Encryption-at-rest af selve arkivet: <ja/nej; tilvalg>
- Restore-tests: separat disciplin.

## 8. Kilder

- BEK nr. 205 af 04/03/2024, § 4, stk. 2.
- <Udbyderens officielle dataresidens-doc>
- <Udbyderens certificerings-side>
- Rentemester-kildehenvisninger: [backup-governance.ts:186-191](../../../src/core/backup-governance.ts#L186).
