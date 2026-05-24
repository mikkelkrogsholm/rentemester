# Backup-destinationer — BEK 205/2024 § 4

Master-guide for hvor og hvordan den ugentlige fulde backup må placeres,
og hvordan en konkret destination attesteres i Rentemester.

## 1. Reglen

### Lovgrundlag

| Kilde | Reference | Hvad den siger |
|---|---|---|
| Bogføringsloven | LOV nr. 700 af 24/05/2022, § 12, stk. 1 | Regnskabsmateriale skal opbevares på betryggende vis i 5 år fra udgangen af det regnskabsår, det vedrører. |
| Bogføringsloven | § 15, stk. 1, nr. 2 | Et digitalt bogføringssystem skal opfylde anerkendte standarder for it-sikkerhed og sikre automatisk sikkerhedskopiering. |
| Bekendtgørelse om ikke-registrerede digitale bogføringssystemer | BEK nr. 205 af 04/03/2024, § 4, stk. 1 | Virksomheden skal mindst ugentligt tage en fuld sikkerhedskopi af alle bogførte transaktioner og bilag — medmindre der ikke er bogført noget siden seneste kopi. |
| BEK 205/2024 | § 4, stk. 2 | Kopien skal opbevares hos en ikke-nærtstående part, der formodes at opfylde anerkendte it-sikkerhedsstandarder, på en server i et EU- eller EØS-land. |
| BEK 205/2024 | § 4, stk. 3 | Stk. 2 gælder ikke, hvis virksomheden allerede er underlagt backup-krav i anden lovgivning. |

### Rentemester-regler

- `DK-BOOKKEEPING-BACKUP-001` ([rules/dk/bookkeeping.yaml:113](../../rules/dk/bookkeeping.yaml#L113))
  — kodificerer ugentligheden og kravet om manifest med timestamps og hashes.
- `DK-BOOKKEEPING-BACKUP-KEY-ROTATE-001` ([rules/dk/bookkeeping.yaml:130](../../rules/dk/bookkeeping.yaml#L130))
  — kodificerer auditerbar rotation af den valgfri Ed25519-signaturnøgle.
- `DK-BOOKKEEPING-RESTORE-001` ([rules/dk/bookkeeping.yaml:185](../../rules/dk/bookkeeping.yaml#L185))
  — kodificerer restore-evnen.

Selve destinationskravet (§ 4, stk. 2) er **ikke** et hard-stop rule_id i
regelsættet i dag. Det håndhæves via governance-flowet i
[`src/core/backup-governance.ts`](../../src/core/backup-governance.ts),
hvor `isCompliantDestination()` kræver at alle tre attestationer er sande
(linje 186-191):

```ts
export function isCompliantDestination(destination: BackupDestination): boolean {
  return (
    destination.regionAttestation.inEeaOrEu === true &&
    destination.nonRelatedParty === true &&
    destination.itSecurityAttestation?.meetsRecognisedStandards === true
  );
}
```

## 2. De tre kriterier

En destination skal opfylde **alle tre** for at være compliant:

| Kriterium | Hvad det betyder | Hvem afgør det |
|---|---|---|
| **EU/EØS-server** | Data-at-rest fysisk i et EU- eller EØS-land. Kontraktlig dataresidens — ikke en best-effort lokalindstilling i en desktop-klient. | Du som menneske, baseret på udbyderens kontrakt eller admin-konsol. |
| **Ikke-nærtstående part** | Backuppen ligger ikke hos dig selv, en ægtefælle, et selskab du kontrollerer, eller en nært forbunden person. Den ligger hos en uafhængig tredjepart. | Du. Konkret: en konsumer-cloud-konto i dit eget navn er fint; et NAS i din egen lejlighed er det ikke. |
| **Anerkendte it-sikkerhedsstandarder (formodningsregel)** | Tredjeparten **formodes** at opfylde standarder som ISO/IEC 27001, SOC 2, ISO 27017/27018. Du behøver ikke aktivt bevise det — men formodningen falder, hvis der er konkrete holdepunkter for at udbyderen ikke gør det. | Du, ved at vælge en udbyder med en kendt certificering. Almindelige Workspace/Microsoft 365/AWS-EU/Hetzner-konti opfylder formodningen uden videre. |

### Om "formodes"

Ordet er bevidst. Det er en juridisk formodningsregel, ikke et "skal":

- Bevisbyrden er vendt — tredjeparten **antages** at opfylde
  standarderne. Du skal ikke kunne fremvise en attest.
- Formodningen kan **afkræftes**: hvis det viser sig at udbyderen ikke
  lever op til anerkendte it-sikkerhedsstandarder (offentligt kendt
  sikkerhedssvigt, ingen kryptering at rest, en privatperson som hoster
  på en VPS), så falder formodningen — og dermed § 4, stk. 2.
- Praktisk: en stor kommerciel udbyder med ISO 27001 / SOC 2 / ENS-attest
  er "safe by default". En ukendt, ucertificeret udbyder er det ikke.

## 3. Sådan attesteres en destination

### CLI

Selve attesteringen sker som en menneske-signeret påstand. Agenten kan
**ikke** selv vide om en cloud-mappe ligger i EU — den flytter filer; du
attesterer rammen.

```bash
bun run rentemester system backup-add-destination \
  --label "<menneskelæsbart navn>" \
  --kind <local-folder|dropbox|google-drive|ssh|other> \
  --location "<lokal sti eller adresse>" \
  --region-eu true \
  --region-country <ISO-3166-1 alpha-2, fx DE> \
  --region-note "<hvor du har attesten fra>" \
  --non-related true \
  --it-security true \
  --it-security-note "<certificeringer, fx ISO 27001/SOC 2>" \
  --attested-by "<dit fulde navn>"
```

Implementeret i [`src/cli/system.ts:143`](../../src/cli/system.ts#L143)
og bagvedliggende `addBackupDestination()` i
[`src/core/backup-governance.ts`](../../src/core/backup-governance.ts).

### MCP

Samme felter via `system_backup_destination_add`-værktøjet (se
[docs/mcp-tool-surface.md:230](../mcp-tool-surface.md#L230)). En agent
kan kalde det, men `attestedBy` og `nonRelatedParty` skal stadig komme
fra en menneskelig instruktion — agenten kan ikke selv attestere kravet.

### Verificering

```bash
bun run rentemester system backup-destinations          # listing
bun run rentemester system backup-governance            # compliance-status
```

Compliance-statussen viser destinationen som `compliant`, hvis alle tre
attestationer er sande. Backup-guide-HTML'en (genereret af
[`src/core/backup-guide.ts`](../../src/core/backup-guide.ts)) markerer
det grønt.

## 4. Livscyklus og kontroltjek

En attestering er ikke evig. Genbesøg destinationen, hvis:

- Udbyderen ændrer dataresidens-politik (fx ny default-region).
- Du skifter Workspace-plan eller M365-plan — nogle dataresidens-features
  er plan-gated.
- En offentligt kendt sikkerhedshændelse rammer udbyderen og rejser
  tvivl om it-sikkerhedsstandarderne.
- Du flytter desktop-klientens mappe til en ny sti.

Anbefalet rytme: tjek attesterne **mindst én gang om året**, og altid
ved fornyelse af regnskabsår. Backup-guiden viser destinationens
attesteringsdato — brug den som påmindelse.

## 5. Konkrete udbydere

- [Google Workspace med Data Regions = Europa](backup-destinations/google-workspace.md)

Tilføj nye guides under `backup-destinations/<udbyder>.md` med
udgangspunkt i [`_TEMPLATE.md`](backup-destinations/_TEMPLATE.md).

## 6. Hvad Rentemester ikke afgør

- **Konsumer-cloud (Google Drive personal, Dropbox Basic, iCloud
  Drive)**: giver ikke kontraktlig dataresidens i EU/EØS. De er ikke
  egnede til § 4, stk. 2 — ikke fordi datacentret nødvendigvis ligger i
  USA, men fordi der ikke er en kontraktlig binding du kan henvise til.
- **NAS hjemme hos dig selv**: fejler på "ikke-nærtstående part".
- **VPS hostet hos en privatperson eller hobbyudbyder uden
  certificering**: formodningen om it-sikkerhedsstandarder kan
  afkræftes — undgå.
- **Encryption-at-rest**: ikke et krav i § 4, men best practice. Rentemester
  signerer i forvejen manifestet med HMAC/Ed25519 (se
  [docs/backup-security.md](../backup-security.md)) — kryptering af
  arkivet selv er din egen beslutning.
