# Completion audit — Bun 1.4, hosted sikkerhed og koncern

Statusdato: 2026-08-24. Dette dokument er et kode- og testbevis, ikke en
produktionsgodkendelse. Ingen release, GHCR-push, produktionstrafik eller
migration af virkelige data er udført.

## Bevist i det aktuelle worktree

| Krav | Autoritativ implementering | Verifikation |
| --- | --- | --- |
| Bun 1.4 | workspace-`package.json`, `Dockerfile`, CI-workflows og fælles `bun.lock` | `bun --version` = 1.4.0; native cockpit-build/test og frozen workspace-install accepteret |
| Reproducerbart OCI-image | `scripts/release/verify-container-reproducibility.ts` | To cache-frie OCI-exports gav samme manifestdigest og arkivhash |
| Versioneret GHCR-flow | release-candidate/promote-workflows og release-manifest | Tests af workflow- og godkendelseskontrakter; ingen ekstern push |
| Lokal én-virksomhedsprofil | `src/cli/local.ts` og deployment-konfiguration | Launcher-, loopback- og profiltests |
| Hosted isoleret workspace | serverkonfiguration og workspace-control DB | deployment-, readiness- og RBAC-tests |
| Individuelt login | Better Auth 1.7.7-adapteren | auth-unit og cookie-E2E |
| MFA og recovery | TOTP, enrollment-gate og engangskoder | challenge-, replay- og revocation-tests |
| Sessionstilbagekaldelse | Better Auth-sessioner og security epochs | logout-all, password/reset og disable-tests |
| CSRF/cookies/rate-limit | central Origin-gate, cookie-policy og proxykontrakt | fail-closed HTTP- og config-tests |
| Medlemskab og RBAC | append-only workspace access-events og central permissionmatrix | cross-company, forkert rolle og route-catalog-tests |
| Bogføringskladder og review | schema v9, append-only kladdeevents og atomisk godkend/postering | core-, CLI-, hosted RBAC- og cockpit-tests |
| Readiness og logs | `/api/ready` og allowlistet request completion-event | read-only, redaction- og sink-fejltests |
| Virksomhedsbackup/restore | signeret backupmanifest og staged restore | checksum-, signatur-, tamper-, restore- og audit-tests |
| Credential-frit workspace-snapshot | `src/core/workspace-snapshot.ts` og `workspace snapshot/restore` | exact allowlist, nested signatures, staged multi-company restore, tamper- og CLI-tests |
| Private dokumenter | membership før opslag, immutable file snapshot og SHA-256 | symlink-, tamper-, cross-company- og adgangsaudit-tests |
| Koncernstruktur | effektivt dateret append-only manifest | interval-, cyklus-, partial-visibility- og CLI-tests |
| Mellemregninger | reviewed eksplicit kontomapping | same-currency, snapshot og dedup-tests |
| Eliminationer | source-bound append-only balanceelimination | four-eyes, stale source og uændrede ledger-hashes |
| Konsolideret rapport | reviewed rapporteringsprofil og read-only rapport | mapping completeness, balance equation, stale elimination og redaction |

Seneste lokale resultat:

- backend: 2.417 tests bestået lokalt med 16 parallelle Bun-workers på 38,09
  sekunder, 0 fejl, 363 filer og 14.624 assertions;
- frontend: 464 tests bestået lokalt med 16 parallelle Bun-workers på 1,87
  sekunder, 0 fejl, 54 filer og 1.118 assertions;
- strict runtime-typecheck: bestået;
- frontend typecheck og produktionsbuild: bestået;
- containerintegration: en helt tom volume blev readiness-grøn via den faktiske
  defaultkommando, første virksomhed blev oprettet gennem API'et, og persisted
  restart samt non-root bestod;
- reproducerbar OCI: bestået;
- `git diff --check`: bestået;
- søgning i ændrede filer: ingen kendte virkelige CVR-numre eller secretmønstre.

## Lukkede kodegates

### Repository-lint

Biome 2.5.10 er pinned som udviklingsdependency og `bun run lint` er blocking i
både almindelig CI og release-candidate-flowet. Den afgrænsede policy scanner
alle 990 TypeScript-/React-kodefiler for højværdi sikkerheds-, korrektheds-,
React-hook-, typekontrol- og ARIA-fejl uden at påtvinge en masseomformatering af
den eksisterende kode. Den afsluttende lintkørsel bestod uden fund eller fixes.

### Dependency- og licensegate (lukket)

Den godkendte dependency-opdatering er udført. `bun audit --audit-level=low`
returnerer ingen advisories i den låste workspace-graf, og React Router er
migreret til 7.18.2. `supply-chain:audit` og `supply-chain:licenses` er
blocking i almindelig CI og release-candidate-flowet.

Den fail-closed licensgate har verificeret 121 produktionspakker under den
eksplicitte allowlist MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause og ISC. Et
deterministisk release-evidensdokument binder det tomme auditresultat,
licensrapporten, Bun-versionen og SHA-256 af den eksakte `bun.lock` til
release-manifestet; promotion verificerer samme hash før udgivelse.

### Hosted workspace-snapshot

Det samlede snapshot er implementeret med den besluttede credential-frie
politik. Hver virksomhed pakkes som en Ed25519-verificerbar backup; et ydre
manifest binder det eksakte filinventar. Navn, e-mail og roller bevares kun i
en privat recovery-plan. Password-hash, sessions, MFA-secret, recovery codes,
reset-/verification-tokens, rate-limit-state og provider-credentials udelades.
Restore publicerer først efter staged verifikation og kræver derefter
ejer-bootstrap og geninvitation; Better Auth-databasen kopieres aldrig råt.

## Eksterne produktionsgates

Disse hører ikke til en lokal kodeændring og må ikke omgås:

- konkret reverse-proxy/TLS- og klient-IP-header-verifikation;
- ekstern adversarial sikkerhedstest;
- DigiSense/compliance-godkendelse af den præcise image-digest;
- menneskelig godkendelse før GHCR-push, release eller produktion;
- særskilt regnskabspolitik før FX, minoritetsinteresser,
  indtægts-/omkostningselimineringer eller lovpligtig koncernrapportering.

`graphify update .` blev forsøgt efter kodeændringerne, men værktøjet nægtede
sikkert at overskrive den eksisterende graf, fordi den nye ekstraktion havde
færre noder end den gemte graf. Der er ikke brugt `--force`.
