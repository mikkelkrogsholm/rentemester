# Compliance

Materialet i denne mappe binder dansk bogførings-, moms-, rente- og
årsregnskabsret sammen med konkret Rentemester-implementering — i to
retninger:

- **Lov → kode → guide**: når du vil vide hvilken paragraf en regel
  kommer fra og hvor den håndhæves i `src/`.
- **System-område → krav**: når du arbejder i en konkret del af systemet
  og vil vide hvilke compliance-krav der rammer den.

Plus konkrete, kildehenviste opskrifter for de dele som *ikke* kan
løses i kode (fx attestering af en backup-destination i EU/EØS).

## Indhold

### Reference

- **[requirements.md](requirements.md)** — Komplet compliance-matrix.
  Én række pr. krav, mappet til lovkilde (med ELI- og XML-link til
  retsinformation), Rentemester rule_id, håndhævelse i `src/`, og evt.
  guide. Den primære kilde til "hvad skal systemet leve op til, og
  hvor er det implementeret?"
- **[requirements-by-area.md](requirements-by-area.md)** — Samme krav,
  men grupperet efter system-område (hovedbog, salgsfakturaer, moms,
  backup, …). Brug denne når du arbejder i en konkret del af koden.
- **[out-of-scope.md](out-of-scope.md)** — Bevidste fravalg: hvilke
  regelsæt Rentemester ikke håndterer (løn, hvidvask, selskabsret,
  cybersikkerheds-direktiver, …) og hvorfor. Lige så vigtigt som
  matricen — forhindrer at en bruger antager dækning, der ikke findes.

### Guides

Rentemester selv kan ikke vide hvor en cloud-mappe fysisk ligger, hvilken
plan en SaaS-konto er på, eller om en udbyder har skiftet datacenter.
Disse guides er rygdækningen for de menneske-attesterede dele af
compliance — den agenten ikke selv kan afgøre.

- [Backup-destinationer (BEK 205/2024 § 4)](backup-destinations.md) —
  master-guide til hvor og hvordan den ugentlige fulde backup må
  opbevares.
  - [Google Workspace (Data Regions = Europa)](backup-destinations/google-workspace.md)
  - [Skabelon for nye udbydere](backup-destinations/_TEMPLATE.md)

## Sådan opdaterer du dokumentationen, når en regel ændres

Når der lægges en ny rule_id til, eller en eksisterende ændrer paragraf-
reference, skal følgende holdes i sync:

1. `rules/dk/<emne>.yaml` — selve reglen med `source_id`, `provisions[]`,
   `severity`, `machine_rule`.
2. `src/core/<modul>.ts` — `const RULE_ID = "DK-…"` så grep let finder
   håndhævelses-punktet.
3. [requirements.md](requirements.md) — tabelrækken i den relevante
   kildesektion + opdater paragrafkolonnen hvis det er en ny henvisning.
4. [requirements-by-area.md](requirements-by-area.md) — tilføj til det
   relevante område-afsnit (eller opret et nyt afsnit hvis emnet er
   nyt).
5. Hvis reglen kræver en menneskelig handling, lav (eller udvid) en
   guide her under `docs/compliance/`.

Når der tilføjes en ny lovkilde (ny LOV/BEK), skal også:

6. [`sources/legal-sources.json`](../../sources/legal-sources.json) —
   tilføj source med `id`, `title`, `url`, `xmlUrl`, `notes`.
7. [`sources/scope.yaml`](../../sources/scope.yaml) — sæt `in_scope`-
   rangen for de paragrafer Rentemester implementerer.
8. Tilføj en ny § X-sektion i [requirements.md § 3](requirements.md#3-matricen-pr-lovkilde).

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
