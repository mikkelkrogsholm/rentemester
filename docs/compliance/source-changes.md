# Lovkilde-ændringer — verifikations-log

Hver gang en lov eller bekendtgørelse Rentemester bygger på bliver
ændret, dokumenteres ændringen her sammen med en analyse af om
Rentemester's regler bliver berørt. Det er det modsatte af et
forudsigelses-værktøj: vi ser tilbage på en konkret ændring og
fastslår hvilke `rules/dk/*.yaml`-regler der skal opdateres (eller, i
de fleste tilfælde, hvorfor de ikke skal).

Logge i kronologisk rækkefølge (nyeste først).

---

## 2025-03-04 — BEK 302/2025 ændrer BEK 1383/2023

**Verificeret:** 2026-05-24. **Verificeret af:** Mikkel Krogsholm.
**Status:** Rentemester's regler er ikke berørt.

### Hvad ændrer BEK 302/2025?

Bekendtgørelse nr. 302 af 04/03/2025 ([ELI](https://www.retsinformation.dk/eli/lta/2025/302))
ændrer bekendtgørelse nr. 1383 af 29/11/2023 om pligt til opbevaring af
bilag i et digitalt bogføringssystem
([ELI](https://www.retsinformation.dk/eli/lta/2023/1383)).

Ordret operativ tekst fra ændringsbekendtgørelsens § 1:

> I bekendtgørelse nr. 1383 af 29. november 2023 om pligt til opbevaring
> af bilag i et digitalt bogføringssystem foretages følgende ændring:
> § 2, stk. 2 og 3, ophæves.

Bekendtgørelsen træder i kraft **1. januar 2026** (BEK 302/2025 § 2).

### Hvad ophæves konkret?

To stykker i BEK 1383/2023 § 2:

**§ 2, stk. 2** (ophæves) — undtagelse for koncernfælles bogføring i
finansielle datterselskaber:

> Stk. 1 finder ikke anvendelse på datterselskaber af virksomheder
> omfattet af lov om finansiel virksomhed, lov om forsikringsvirksomhed
> eller lov om Arbejdsmarkedets Tillægspension, hvis disse anvender et
> koncernfælles bogføringssystem.

**§ 2, stk. 3** (ophæves) — indfasnings-/anvendelses-bestemmelser:

> Bekendtgørelsen har kun virkning for virksomheder, som er
> bogføringspligtige efter bogføringslovens § 1, stk. 1, og som efter
> årsregnskabslovens § 3, stk. 1, har pligt til at aflægge en årsrapport,
> for førstkommende regnskabsår, der starter fra og med
>   1. 1. juli 2024, hvis virksomheden bruger et registreret standard
>      bogføringssystem, …
>   2. 1. januar 2025, hvis virksomheden bruger et ikke-registreret
>      bogføringssystem, …

Stk. 3 har naturligt udtømt sin funktion: indfasnings-datoerne 2024-07-01
og 2025-01-01 er overstået på det tidspunkt, ændringen træder i kraft.

### Hvad citerer Rentemester af BEK 1383/2023?

Et grep efter `§ 2` mod kilden `DK-BILAG-OPBEVARING-2023-1383` i alle
YAML-regler returnerer **ingen match**. Rentemester citerer kun § 1
(stk. 1, nr. 1-6, stk. 2, stk. 3), og de paragraffer er **ikke** rørt
af ændringen.

Konkret berører ændringen:

- `DK-DOCUMENT-STORAGE-001` — citerer § 1, stk. 1, nr. 1-6 → uændret.
- `DK-DOCUMENT-CASH-RECEIPT-001` — citerer § 1, stk. 2 → uændret.
- `DK-DOCUMENT-FOREIGN-PHYSICAL-001` — citerer § 1, stk. 3 → uændret.
- `DK-MAIL-INTAKE-EXCEPTION-001` — citerer § 1, stk. 1 → uændret.
- `DK-MASTER-DATA-VENDOR-001` — citerer § 1, stk. 1, nr. 4 → uændret.

### Konsekvenser for Rentemester's målsegment

Selvom Rentemester's regler ikke ændres, er det værd at notere:

1. **Finansielle datterselskaber med koncernbogføring** er ikke længere
   undtaget fra digital bilag-opbevaring fra 2026-01-01. Det er en
   stramning, men ikke relevant for Rentemester's målsegment (almindelige
   danske SMB'er, ikke koncern-datterselskaber af FIL/forsikrings-
   virksomheder).
2. **Indfasnings-datoerne** er ophævet fordi de er udtømte. Rentemester
   har siden lanceringen opereret som om kravet var fuldt gældende —
   ingen ændring.

### Konklusion

Ingen rule_id eller text_hash skal opdateres som konsekvens af BEK
302/2025. `sources/legal-sources.json` har allerede ændringsbekendt-
gørelsen registreret med en accurate `notes`-feltbeskrivelse.

### Hvis ændringen alligevel skal håndteres i fremtiden

Hvis Rentemester en dag skal kunne servere finansielle datterselskaber
(højst usandsynligt — kræver registrering som finansielt
bogføringssystem), så skal § 4 stk. 3 i BEK 205/2024 ([backup-undtagelse
hvis underlagt anden lovgivning](backup-destinations.md)) genvurderes
sammen med denne ændring. Det er en hypotetisk fremtids-overvejelse, ikke
en aktiv pligt.

---

## Sådan tilføjer du et nyt log-eksempel

1. **Find ud af om en kilde i `sources/legal-sources.json` er ændret.**
   I praksis: når Erhvervsstyrelsen, Skatteministeriet eller en anden
   myndighed udsender en ændringsbekendtgørelse til en lov eller bek
   Rentemester citerer.
2. **Tilføj/opdatér kilden i `sources/legal-sources.json`** med
   ændringsbekendtgørelsens egen ID, URL og XML-URL.
3. **Download den nye XML** med `bun run scripts/download-legal-sources.ts`.
4. **List den ændrede bekendtgørelses operative bestemmelser** med
   `bun run scripts/lookup-provision.ts <SOURCE-ID> "§ 1"` (eller list
   alle med en lille variation af scriptet).
5. **Læs ordret hvad der ophæves/ændres**, og match mod hvilke regler
   der citerer den affekterede paragraf.
6. **Skriv et nyt § X-afsnit øverst i denne fil** med:
   - Dato for ændringen og ikrafttrædelse
   - Ordret tekst af ændringen
   - Liste over påvirkede `rule_id` (eller "ingen — vi citerer ikke
     de ændrede paragrafer")
   - Beslutning: opdater regler / opdater dokumentation / ingen handling
7. **Hvis regler påvirkes:** opdater den relevante `rules/dk/*.yaml` med
   nye `text_hash`-værdier (via `scripts/lookup-provision.ts`) og
   tilsvarende rækker i [requirements.md](requirements.md).
