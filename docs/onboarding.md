# Kom i gang med Rust-udgaven

Rentemester bruger én Rust-motor og server. Web og Rust/Tauri-desktop viser det
samme React-UI; CLI og MCP bruger serverens adgang og regnskabskontroller.
Den samlede release-, flytte- og installerede desktopaccept er stadig åben i
[epic #672](https://github.com/mikkelkrogsholm/rentemester/issues/672).
Brug en særskilt syntetisk virksomhed til afprøvning. Overflyt ikke den eneste
rigtige ledger, før de oprindelige release- og cutovergates er godkendt.

## Vælg hvor regnskabet skal køre

| Situation | Start her | Hvem gør hvad? |
| --- | --- | --- |
| Lokal desktop | Appens **Opret lokalt regnskab** eller **Åbn lokalt regnskab** | Ejeren bruger fælles UI; appen ejer den lokale Rust-server. |
| Desktop mod server | Appens **Forbind til server** med den aftalte HTTPS-adresse | Operatøren driver serveren; ejeren logger ind og vælger virksomhed. |
| Web mod server | Åbn operatørens HTTPS-adresse | Samme login, MFA, virksomheder og brugerforløb som i desktop. |
| CLI eller agent | [CLI-kontrakt](cli-contract.md), [MCP-installation](mcp-install.md) | Aktuel bruger eller eksplicit begrænset servicekonto, samme server. |

Kontrollér den synlige **Server:**-adresse og den valgte virksomhed, før du
arbejder. En forbindelsesfejl skifter ikke automatisk til et lokalt regnskab.
En mappekopi er ikke en understøttet flytning af et eksisterende regnskab.

## Første ejer og første virksomhed

På en ny lokal installation vælger ejeren **Opret lokalt regnskab** og følger
første-ejerforløbet. På en server opretter operatøren først det nye workspace
med en privat konfiguration og den eksplicitte setup-operatør; ejeren bruger det
private installationslink. Se [serverens opsætningskontrakt](rust-server.md#aktuel-afgrænsning).
Et færdigt setup genåbnes ikke ved en almindelig servergenstart.

Verificér e-mailen og tilmeld MFA gennem det almindelige UI. Ved lokal drift
findes det private bekræftelseslink gennem appens menu **Lokal drift**; en
HTTPS-server bruger sin konfigurerede maillevering. Opbevar recovery codes
privat. Der gives ikke adgang ved at skrive et actor-navn eller en e-mail i en
klientkonfiguration.

Vælg virksomhed, kontrollér virksomhedsoplysninger, regnskabsår og momsforhold,
og gennemgå kontoplanens roller og bankkonti. Brug virksomhedens faktiske
kontoplan og dokumenterede mappings; et eksempelkontonummer er aldrig en
produktstandard. Ekstra brugere inviteres og tildeles de nødvendige aktuelle
rettigheder under **Brugere og adgang**. En servicekonto skal også have
udtrykkelig virksomhedsadgang; oprettelse af en nøgle alene giver den ikke.

## Ny virksomhed eller eksisterende regnskab

For en ny virksomhed gennemgås åbningsgrundlag og kontoroller før den første
postering. Et eksisterende regnskab skal gennem den dokumenterede
[import](rust-dinero-import.md) eller [flytning](rust-server.md#flyt-workspace-mellem-lokal-drift-og-server).
Gennemgå kildefiler, plan, regnskabsidentiteter og kontrolresultater før apply.
Genbrug ikke en gammel serverkonfiguration eller kopieret datamappe som en ny
aktiv skriver. Operatøren håndterer migration, kompatibilitet og det præcise
mål; ejeren godkender det regnskabsmæssige grundlag.

Native schema 31 kræver en særskilt, stoppet og autoriseret
[custody-enrollment](architecture/rust-target/implementation/namespace-custody.md)
ved opgradering til fysisk schema 32. Normal opstart udfører den ikke.

## Første verificerede bogføring

1. Vælg korrekt server, virksomhed og periode. Kontrollér regnskabets
   [backupstatus](rust-backup-destinations.md) og eventuelle spærrer.
2. Indlæs det originale bilag. Gennemgå dato, valuta, beløb, momsgrundlag,
   modpart og de eksplicit valgte konti i det relevante bilagsforløb.
3. Gennemgå serverens forslag og bekræft den konkrete handling én gang.
   Udstedelse, bogføring, betaling og afsendelse er særskilte beslutninger.
4. Genlæs posteringen og originalen; kontrollér journal, bank og relevante
   rapporter. En fejl rettes gennem det dokumenterede korrektionsforløb,
   ikke ved at redigere eller slette historik.
5. Hvis et svar forsvinder, afklar den eksisterende operation og dens
   kvittering før en ny handling. Gentag ikke bogføring eller afsendelse
   med en ny nøgle for at se, om det virker.

Se [regnskabskontrol](rust-audit-verify.md) og [revisionshistorik](rust-audit-log.md).
Gem en signeret backup og den separat betroede offentlige nøgle. Kontrolleret
fresh recovery, ikke blot download af en fil, er beviset for gendannelse.
[Ejer- og operatørhjælpen](rust-support.md) beskriver fejl, flyttede data og
hvilke oplysninger der sikkert kan deles med support.

## Installation for udvikleren og operatøren

Brug en identificeret releasekandidat og dens dokumenterede platform; en
usigneret lokal build er ikke en godkendt distribution. Operatøren følger
[serverkontrakten](rust-server.md), [releaseguiden](release/README.md) og de
oprindelige kompatibilitets-/cutovergates før adgang til rigtige data.

Ved udvikling bruges repositoryets fastlåste Rust-toolchain og mindst 30 GiB
ledig plads på kilde- og byggevolumen. Eksempel fra repositoryroden:

```sh
python3 scripts/rust/cargo-guard.py status
python3 scripts/rust/cargo-guard.py cargo build --locked \
  -p rentemester-server -p rentemester-cli -p rentemester-mcp
```

Bun bruges fortsat til React-build og eksisterende udviklings-/legacyværktøjer:

```sh
bun install --frozen-lockfile
cd app
bun run build
```

Cargo-wrapperens status angiver den valgte outputmappe; antag ikke automatisk
`target/debug` ved en eksplicit eller fælles cache. Brug de identificerede
native binærfiler fra builden eller pakken. Kontrollér serverens `--build-info`,
CLI'ens `--help` og MCP'ens `--help`/`--catalogue` uden virksomhedsændringer.

Repositoryets `bin/rentemester` vælger Rust-binæren og bygger eller skifter ikke
motor ved fejl. `RENTEMESTER_LEGACY_CLI=1` er en eksplicit historisk opt-in.
`bin/rentemester-mcp` er endnu Bun-wrapperen: konfigurer derfor den konkrete
native MCP-binær efter [MCP-guiden](mcp-install.md). Den endelige fjernelse af
den erstattede runtime afventer #727s fulde parity/release/cutoveraccept.
