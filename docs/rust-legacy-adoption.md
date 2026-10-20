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

Gruppe 89 kvalificerede det første fakturakorpus. Gruppe 90 nedenfor færdiggør
fortsat native bogføring og signeret, kildefri gendannelse for det udvidede korpus.

## Bogfør videre, tag backup og gendan — gruppe 90

To faktiske TS-virksomheder oprettes og får hver en udstedt og bogført faktura;
den ene arkiveres gennem den eksisterende TS-operation. Efter overtagelsen
opretter den nye ejer en native postering, genåbner workspacet og gentager samme
kald uden en ny postering. Backup dannes og downloades gennem den eksisterende
webknap og indeholder begge ledgers, alle fire originaler og arkivflaget.

Begge kilder og signeringsnøglen fjernes, før den separate offentlige nøgle
bruges til uafhængig native verifikation. Et nyt fysisk workspace gendannes med
**samme logiske workspace-id som backupen**, nye nøgler og en ny ejer med normal
bekræftelse/MFA. Gamle posteringer, originaler, den arkiverede ledger og den
fulde adoptionskvittering bevares. Det tidligere native bogføringskald afvises
med `HTTP400 RESTORED_KEY_RECONCILIATION_REQUIRED`; et nyt eksplicit kald
bogføres, og genstart/gentagelse bevarer resultatet.

Slutresultatet er tre journaler/syv linjer i den aktive virksomhed og én
journal/tre linjer i den arkiverede. Debet og kredit er henholdsvis 270/270 og
50/50. Alle fire oprindelige bilagsfiler er bevaret. Alle prøveprocesser er stoppet.

Arkiverede virksomheder kan kun besøges gennem den interne snapshot-adgang.
Backup og flyttestatus vender tilbage til almindeligt valg af den aktive
virksomhed, også efter fejl; et mislykket tilbagevalg pensionerer ejeren.
Almindelige forespørgsler på arkiverede virksomheder afvises fortsat, og backup
kræver aktuelle administratorrettigheder til hver virksomhed, også arkiverede.

Reproducerbar prøve:

```sh
python3 scripts/rust/run-legacy-adoption.py \
  --server-binary target/debug/rentemester-server --web-dir app/dist \
  --evidence /tmp/rentemester-rust-reports/NEW-unique-evidence.json
```

Prøven bruger udelukkende et nyt, privat syntetisk workspace og overskriver ikke
eksisterende evidens. Gruppe 90-evidensen er
`/tmp/rentemester-rust-reports/legacy-adoption90-accounting-sourcefree-v2.json`.
Den registrerer også den første UI-afvisning samt rettelser af prøvekonfiguration
og forventet fejlkode. Den afsluttende gendannelse fortsatte fra den allerede
verificerede backup uden at gentage gennemførte kildeoperationer.

Dette er accept af det beskrevne korpus og forløb. J03 — installeret desktop mod
en fælles server med flere brugere, adgangstab og usikre svar — er næste
samlede acceptgrænse. Linux-drift, signerede udgivelser, opdateringer og den
samlede udfasning af TS-backenden er fortsat åbne dele af Rust-omskrivningen.
