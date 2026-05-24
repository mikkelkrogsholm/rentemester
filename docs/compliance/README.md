# Compliance-guides

Praktiske, kildehenviste guides til hvordan en konkret opsætning lever op
til de regler Rentemester håndhæver. Hver guide tager udgangspunkt i:

1. **Hvilken regel** (lov + paragraf + Rentemester-regelid)
2. **Hvad reglen kræver** ordret eller parafraseret
3. **Hvordan en konkret opsætning opfylder kravet**
4. **Hvilke CLI-/MCP-kald** der attesterer det i Rentemester
5. **Hvad der kan vælte attesteringen senere** (kontroltjek)

Rentemester selv kan ikke vide hvor en cloud-mappe fysisk ligger, hvilken
plan en SaaS-konto er på, eller om en udbyder har skiftet datacenter.
Disse guides er rygdækningen for de menneskeskabte attestationer som
agenten kan stole på.

## Indhold

- [Backup-destinationer (BEK 205/2024 § 4)](backup-destinations.md) —
  hvor og hvordan den ugentlige fulde backup må opbevares.
  - [Google Workspace (Data Regions = Europa)](backup-destinations/google-workspace.md)
  - [Skabelon for nye udbydere](backup-destinations/_TEMPLATE.md)

## Sådan tilføjer du en ny guide

1. Find den relevante master-guide (fx `backup-destinations.md`) og se
   hvilke felter en udbyder-guide skal udfylde.
2. Kopiér den nærmeste skabelon (fx `backup-destinations/_TEMPLATE.md`)
   til en ny fil med udbyderens navn i kebab-case:
   `backup-destinations/<udbyder>.md`.
3. Udfyld alle felter med konkrete tal, links og admin-skærmnavne — ikke
   "tjek hos udbyderen". En guide er kun værd, hvis en ny bruger kan
   følge den uden at ringe til Google's support.
4. Tilføj filen til indholdsfortegnelsen ovenfor og i master-guidens
   "Konkrete udbydere"-afsnit.
5. Hvis guiden afdækker en udbyder som **ikke** lever op til kravet,
   skriv det også som en guide og marker den `status: ikke-egnet` i
   frontmatter-blokken. En liste over ikke-egnede valg er ofte mere
   værd end listen over egnede.

## Kildediscipln

- Lovtekst citeres ordret med paragrafhenvisning.
- Bekendtgørelsens nummer og dato skal stå i hver guide (fx
  "BEK nr. 205 af 04/03/2024").
- Udbyder-fakta (dataresidens, certificeringer, planpriser) skal linke
  til udbyderens officielle dokumentation, ikke til blogposts.
- Datér guiden i frontmatter-blokken og opdatér datoen, når du ændrer
  fakta — udbyderindstillinger flytter sig.
