# Flyt et TypeScript-workspace til Rust

Rust-serveren har en eksplicit operatørkommando til at overtage et eksisterende
workspace. Det første faktiske acceptforløb bruger en syntetisk virksomhed, som
TypeScript selv har oprettet, udstedt en faktura for og bogført.

Forbered en ny privat serverkonfiguration med et nyt destinationsworkspace,
nye cookie-/MFA-nøgler og en ny `firstOwner` eller `setupActor`, som står i
`operatorActors`. Mail- og afsendelsesudbydere skal være slået fra under overtagelsen.
Kilde og destination skal angives som absolutte, kanoniske stier; destinationens
mappe må ikke findes, og dens overmappe skal findes.

Stop og dræn først alle gamle servere, CLI-/MCP-processer, jobs og eksterne
operationer. SQLite-kontrollen kan ikke bevise, at en gammel netværksoperation
er afsluttet. Afklar uafsluttede afsendelser og udtræk, før overtagelsen fortsætter.

```sh
rentemester-server --adopt-legacy /absolute/source \
  --config /absolute/new-private-server.json \
  --confirm-target /absolute/new-workspace \
  --legacy-source-stopped
```

Kommandoen viser først det fastholdte datagrundlag, originalernes mapping og
review-hash. Indtast den præcise hash for at pensionere kilden. Kildens databaser
får varige skrivehegn, også mod allerede åbne SQLite-forbindelser. Et efterfølgende
fejltrin kan efterlade nogle databaser pensioneret; fejloutput viser fremskridtet.
Der er ingen automatisk genaktivering af kilden.

Indtast derefter den præcise hash for det pensionerede review for at publicere
destinationen. Operationen er bundet til denne destination. Publicering overskriver
aldrig et eksisterende workspace. Eksisterende kodeord, sessioner, MFA, API-nøgler,
medlemskaber og udbyderkonfiguration overføres ikke som adgangsautoritet.
Originale bogføringer, hashes, bilag og historisk aktørproveniens bevares.

Start herefter serveren normalt med samme nye konfiguration. Den nye ejer bruger
det eksisterende forløb med e-mailbekræftelse og totrinsbekræftelse. En auditeret
adoptionskvittering fastholder reviewet; den forveksles ikke med en signeret native
backup. Genstart med `firstOwner` er idempotent, og en senere native gendannelse
har sin egen adgangsgrænse.

## Verificeret grænse, gruppe 89

Det faktiske TS-forløb bevarer én bogført faktura, én journal med tre linjer,
original JSON og PDF samt journalhash. To præcise operatørreviews, pensionering,
publicering, almindelig ny ejer/MFA og nyt login efter genstart består. Den
oprindelige PDF downloades med samme SHA256. Gamle TS-åbninger og manifestændringer
afvises. Alle prøveprocesser er stoppet.

Evidens: `/tmp/rentemester-rust-reports/legacy-adoption89-first-actual.json`.
Storage-, runtime-, host- og bekræftelsestests samt streng Clippy består. Den brede
TS-typekontrol har fortsat fem fejl i den uændrede `check-native-dashboard.ts`.

Dette kvalificerer det første fakturakorpus. Fortsat native bogføring efter
migration, signeret backup/gendannelse af migrerede data og bredere kildedata
mangler samlet faktisk accept. Arkiverede virksomheder bevares i adoptions-
testen, men den eksisterende native backupvalidator afviser endnu `archived:true`.
Linux-drift, signerede udgivelser, opdateringer og fuld udfasning af TS-backenden
er fortsat åbne dele af den samlede Rust-omskrivning.
