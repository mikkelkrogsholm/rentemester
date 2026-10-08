# Changelog

Alle væsentlige ændringer i Rentemester dokumenteres her. Formatet følger
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), og produktversioner
følger [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.3.0] - 2026-10-08

### Added

- Vedvarende workspace-opgaver med selskabs- og fælles scope, historik,
  afslutningsbeviser, optimistisk versionskontrol og idempotente mutationer.
- Samme opgaver i liste, kalender, kanban og årshjul; egne kolonner,
  periodebaserede rutiner og beskyttede links til kilder og playbooks.
- Komplette kildeadaptere til bankarbejde, undtagelser, periodelukning,
  registreret moms og godkendelsesbehov med synlig uafklaret dækning.
- Brugeraktiverede påmindelser i produktet, minutrunner, leveringshistorik og
  genstart uden dubletter. Ingen mail eller automatisk agentarbejde.
- Fælles HTTP-, CLI- og MCP-kontrakter, adgang før counts/søgning/kvitteringer
  samt backup/restore af opgaver, boards, rutiner og dokumentation.

### Security

- Adgang genkontrolleres i mutationstransaktionen og ved genbrug af tidligere
  resultater. Historiske scopes og samlet board-preview afslører ikke arbejde
  fra selskaber, som brugeren ikke kan tilgå.
- Opgavestatus kan ikke bogføre, betale, indberette eller omgå afslutningsbevis.
  Genkendelige CPR-/bankoplysninger afvises i kortets fritekst.

## [0.2.0] - 2026-10-03

### Added

- Hosted login med Better Auth, TOTP-MFA, recovery codes, sessionslukning og
  server-side adgangskontrol på tværs af virksomheder.
- Roller, invitationer, virksomhedsskifter og revisionsspor for handlinger i
  webinterfacet.
- Kontrollerede bogføringskladder, review, dokumenthåndtering, snapshots og
  generiske health-, readiness- og restore-kontroller.
- Koncernstruktur, mellemregning, elimineringer og read-only konsolidering,
  mens hver juridisk enhed beholder sin egen ledger.
- Bun 1.4-native cockpit-build og et reproducerbart, non-root OCI-image med
  release-evidens, SBOM og attestering.
- Fælles StyleX-komponenter, genererede designtokens, lokale fonts og et
  komplet komponentkatalog i `DESIGN.md`.
- Sidebjælke og mobilnavigation fra det kanoniske ruteregister samt egne
  sider til fakturaoprettelse, dokumentdetaljer og bilagsbogføring.

### Changed

- Den fulde backend- og cockpittestkæde køres lokalt og parallelt via
  `bun run verify:local`; GitHub verificerer build, containerkontrakt og
  reproducerbarhed og publicerer kandidat-imaget uden at gentage tusindvis af
  domænetests.
- Produktet kan køre både som enkelt lokalt workspace og som hosted løsning
  med flere virksomheder og flere ejere.
- Cockpittet accepterer den dokumenterede `local-container`-profil som lokal
  drift uden Better Auth; release-gaten renderer nu den publicerede profil i
  en rigtig headless browser.
- Daglige lister bruger fælles filtre, dansk beløbsinput og responsive tabeller.
- Native dialoger styrer fokus, inert baggrund og beskyttelse af ugemte felter.
  Afbrudte skrivninger blokeres, indtil deres resultat er kontrolleret.
- UI-integrationen bevarer nyere partscoverage, importerede tilgodehavender,
  eksakt plan-godkendelse, kanoniske regnskabsår og fysisk read-only læsninger.
- Fælles API-transport, fakturaserialisering, refresh, CLI-policy-routing og
  transaktionsafvisning er samlet uden ændring af de understøttede workflows.
- MCP deler en kontrolleret read-only ledger-livscyklus; den ubrugte
  Dinero-importgren og runtime-agentens afhængighed af CLI-formatering er fjernet.

### Fixed

- MCP-tests kontrollerer den faktiske registrerede toolsurface og bruger
  read-only observationer; CLI-tests vælger en ledig port.
- Backup-testen synkroniserer med den erhvervede databaselås, så langsom
  procesopstart ikke giver en falsk fejl.
- Partsøgning bevarer input og fokus under skift mellem læsetilstande.
- Kvitteringen for «Markér som set» bevares, når ændringslisten bliver tom.
- Mobilknapper ombrydes, og navigationens disclosures har selvstændige,
  tilgængelige udløsere.
- Runtime-agenten bogfører ikke lige stærke bilagsmatches; de sendes til review.
- Ugyldige journalfelter og beløb, der bliver uendelige ved afrunding, afvises
  kontrolleret uden bogføring, audit- eller sekvenseffekter.
- Afskrivning og straksafskrivning før anskaffelsesdatoen afvises.
- Fakturabetaling, refundering og kravbetaling kræver entydige bankreferencer
  eller et eksplicit bank-ID; modstridende ID/reference afvises.
- ZIP-import rydder egne ekstraktionsmapper ved succes, afvisning og exceptions.
- Backup og restore kræver præcis overensstemmelse mellem dokumentregister
  og evidensfiler, inklusive hashes; manglende, ændrede og uregistrerede filer afvises.
- En sen revisoreksport starter ikke download efter navigation væk fra virksomheden.

### Security

- Tilføjet fail-closed virksomhedsskel, rolle- og MFA-kontroller, sikre
  cookies, CSRF-beskyttelse, login-rate-limit og private dokumentdownloads.
- Dependency-, licens- og containerkontroller indgår i release-gaten.
- Opdateret sårbare transitivt installerede versioner af `fast-uri`, `hono`
  og `ip-address`; dependency-auditten kræver fortsat nul advisories.
- Virksomhedsresolveren afviser symlinks uden for workspacet, aliaser til andre
  virksomheder og dangling symlinks.
- Koncernforslag skrives først efter actor-kontrol for begge virksomheder.
- Knowledge- og ownership-CLI håndhæver alle berørte virksomheders allowlists
  for både eksplicitte og miljøudledte actors. HTTP/MCP-review og apply udleder
  adgangsscope fra det gemte snapshot frem for indsendte facts.
- Beskyttede browserskrivninger gemmer og verificerer en fælles blokering før
  requesten sendes. Ukendte udfald forbliver blokeret på tværs af faner; gamle
  blokeringer uden brugeridentitet bevares indtil eksplicit afklaring.

## [0.1.0] - 2026-07-19

### Added

- Første kanoniske SemVer på tværs af CLI, MCP, HTTP API og cockpit.
- Build-identitet med Git-commit og deterministisk buildtid i release-images.
- Checksummet schema-baseline med afvisning af databaser fra nyere software.
- Deterministisk SHA-256-identitet for regler og juridiske kildefiler.
- Versions- og regelproveniens i backups, myndighedseksport og SAF-T-eksport.
- Multi-stage, non-root Docker-image med cockpit, runtime, regler og kilder.
- To-trins GHCR-releaseflow: kandidatimage og digest-bundet Digisense-promovering.
- Selvstændig CI for det separate `www`-site.

### Changed

- CI bruger den fastlåste Bun-version 1.3.14 i stedet for `latest`.
- Docker Compose-eksemplet kræver et eksplicit digest-pinnet image og bruger
  aldrig `latest`.

[Unreleased]: https://github.com/mikkelkrogsholm/rentemester/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/mikkelkrogsholm/rentemester/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/mikkelkrogsholm/rentemester/releases/tag/v0.1.0
