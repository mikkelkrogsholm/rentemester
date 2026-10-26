# Autonomous closeout after #716

User mandate: finish the remaining work without repeated “continue” messages, one
existing issue at a time, and close whole issues immediately when accepted. GitHub
remains delivery authority; this is a dependency/acceptance plan, not an executable
queue. SGD owns only the current finite increment.43 of55 original issues are
confirmed closed at6b784b41; all55 scopes and acceptance criteria remain binding.

## Next order

| Order | Existing issue | Work that can proceed without the user | Closure boundary |
| --- | --- | --- | --- |
| 1 | #718 React cockpit | Reuse accepted actual browser journeys; finish missing original screens/file/error/accessibility journeys against the existing Rust server. | All4 original browser criteria; mocked API or React unit tests alone cannot qualify it. |
| 2 | #725 server package | Reconcile existing image/provenance evidence; finish only missing fresh-volume, proxy, constrained runtime and recovery proof. | Exact reviewed bytes and existing human/provider promotion gates remain binding; no production promotion is inferred. |
| 3 | #719 desktop lifecycle | Finish any remaining Rust/Tauri lifecycle, explicit workspace/onboarding and recoverable sidecar failure paths using the same React UI. | Installed offline/native acceptance remains in the final desktop pass. |
| 4 | #720 desktop readers | Finish missing shared read entrypoints, data limits, safe switching and backup/status/error semantics. | Real Tauri accessibility/preview/installed parity at the final desktop pass. |
| 5 | #722 remote desktop | Finish any remaining profile switch, cancellation, moved-source and operation-specific reconnect/read-back behavior. | Same installed Tauri client/server ledger and native profile/cookie validation at the final desktop pass. |
| 6 | #721 desktop writes/admin | Finish remaining shared React setup/review/bookkeeping/invoice/VAT/backup paths and stale/unknown/refusal handling. | Complete installed J01/J02/J05 without JSON/CLI; never substitute old GPUI or mocked proof. |
| 7 | #724 packages/updates | Prepare exact package manifests, compatibility/update interruption and signature-refusal checks. | Real available signing/notarization and claimed-platform install tests; unavailable credentials remain an explicit blocker. |
| 8 | #723 cross-interface parity | Compose valid operation/scenario proofs; run only missing full-year/restart/restore/concurrency negatives. | J01–J07, genuine qualified-platform proof and required human/independent/provider reviews. |
| 9 | #726 cutover rehearsal | Synthetic source fencing, transfer, corrupt/failed activation, restore and rollback rehearsal; preserve uncertain effects. | No live switch; all upstream gates plus measured recoverability and full original criteria. |
| 10 | #727 retirement/support | Prepare accurate runtime/install/recovery/support documentation. Remove the legacy backend only after the original coverage/cutover/release gates permit it. | No permanent hidden TypeScript runtime; no premature deletion or cutover. |

#678 (selected Tauri host/platform/accessibility qualification) and #712 (final
Tauri auth integration) are retained as explicit final desktop gates. Current
#712 auth backend is accepted and used by the web/server; its remaining exact
Keychain→WebKit/menu/close→reopen/cancel proof is not a missing backend to rewrite.
This accepted backend contract enables independent web/server work; the original
GitHub dependency and native acceptance are not removed or falsely closed.

## How the series advances

Keep one product flow active. Before every SGD transition identify the missing
original criterion, its smallest necessary action, and whether the issue can now
close. A passing whole issue is reviewed, published and closed in the same round.
A solely manual/external final gate is recorded precisely while preserving the
completed code/proof; continue only work independent of that unavailable gate.
Never mark a partial issue done to unlock the next label. Re-evaluate this order
when concrete evidence changes which whole issue is closest to closure.

Reuse existing fixtures, binaries and source-bound evidence. At15minutes of test
setup or two setup failures, stop growing the setup, diagnose one cause and choose
a simpler valid proof or record the precise external blocker. No new harness,
extra manager, repeated broad tests or optional polish. The same common Rust
motor/server serves web and local/remote Rust/Tauri with the same React UI.

The source/output30GiB compiler guard remains binding. Only this mission’s idle,
rebuildable output and finished worktrees may be removed after preserving code,
receipts and reports. Do not clear another project’s active builds or user data.

Use the existing thread heartbeat as recovery while this authorized series runs.
It must reconcile actual Git/SGD/processes/receipts before acting, preserve stops
and accepted effects, and notify only for a completed whole issue, meaningful
change, actual blocker or necessary user action. No unattended manual-desktop
requests, production promotion, live migration or self-certification of external
review. Stop the autonomous series when only those unavailable gates remain or
the user stops it; do not keep polling an unchanged blocker.

Browserdelen af #718 er nu accepteret med kilde- og binærbundet bevis i `issue718-evidence.json`: faktisk lovgrundlag, filer, tastatur, fejl, CFO-koncern/paginering og de manglende metadatahandlinger. Hele issuet holdes åbent for den oprindelige J03-nativeaccept og #712; installeret Tauri-accept ligger fortsat sidst. Næste udførbare arbejde er #725s uafhængige serverpakkeaccept mod den accepterede web/server-kontrakt; dette ophæver ingen GitHub-afhængighed eller menneskelig/provider-promotiongate.

Den aktuelle LinuxARM64-serverpakke i #725 har nu faktisk bestået pakkeidentitet, isoleret bogføring/filer/genstart, HTTPS/MFA/password recovery og signeret gendannelse til frisk server. `issue725-evidence.json` bevarer de konkrete resterende AMD64-/reproducerbarheds-/registry-/review-/promotionkrav og nativeafhængigheder; ingen produktionspromotion er foretaget. Næste uafhængige produktforløb triageres fra #719.

#719s konkrete Managed-ejerfejl er nu rettet og prøvet mod den aktuelle Rust-server: to ejere, servercrash, optaget port og faktisk SIGKILL af ejerprocessen efterfulgt af automatisk serverstop og normal genåbning. `issue719-evidence.json` bevarer alle fem originale kriterier og slutgates; fuld installeret J01/J02/J07/offline/nativeaccept ligger fortsat sidst. Næste konkrete uafhængige læsehul i #720 er journalens ubundne tabelrendering; originalt virtualiseringsscope og målte platformgrænser er stadig åbne.

#720s manglende virtuelle journal er nu implementeret i det fælles React-UI og målt med10.000 syntetiske posteringer/20.000linjer iChromium, under40monterede rækker, bevaret kladde/fokus og skift uden fremmede data. Den aktuelle produktions-UI er desuden læst mod det bevarede reelle Rust-testregnskab uden nye bogføringer. `issue720-evidence.json` skelner disse beviser fra de oprindelige installerede/native/platformkrav, der stadig ligger sidst. Næste påviste lille rest i #722 er tydelig visning af aktiv server efter forbindelse; ingen ny profilmanager er nødvendig.

#722s konkrete visning af aktiv server er nu implementeret og faktisk prøvet mod Rust med normal MFA-bruger; `issue722-evidence.json` bevarer alle fire originale kriterier og slutkrav. #721 har efter målrettet kildetriagering alene sin oprindelige installerede J01/J02/J05-slutaccept tilbage. Næste nødvendige uafhængige leverance er #724s aktuelle staged Mac-pakke via eksisterende builder; derefter #723s allerede eksplicit udestående source-free restore af det bevarede reported-momsarkiv, uden ny bogføring eller registrering.

#724s aktuelle Mac ARM64-kandidat er nu bygget via den eksisterende builder med pin1.94.1-server/updater, pin1.99.1-Tauri, fælles aktuelle React-UI og appikon. Alle13 manifestfiler og tre binærer er identificeret; faktisk Managed-ejer/crash/genåbning er prøvet mod pakkens server. Pakken er bevaret til sluttesten. `issue724-evidence.json` fastholder alle originale signerings-, platforms-, opdaterings- og installerede J07-gates. Næste konkrete rest er #723s bevarede reported-momsarkiv: verificér og gendan til frisk isoleret mål uden kilden, læs reference/bogføring/originaler/audit efter normal ejer-MFA og genstart; ingen ny bogføring eller registrering. Derefter #726s oprindelige ikke-tomme job-/fejl-/rollback-matrix gennem eksisterende transferforløb.

#723s eksplicit udestående R17 reported-momsarkiv er nu verificeret og gendannet på et frisk isoleret Linuxmål uden kildeadgang. Ny normal ejer/MFA, præcis reference/pakkehash, bogføring/moms/originalbilag, de17oprindelige audithændelser og read-only genstart er prøvet; ingen bogføring eller registrering blev gentaget. `issue723-evidence.json` bevarer de originale fulde platform-/native-/J01–J07-/menneskelige/eksterne slutgates. Næste udførbare #726 bruger eksisterende transferforløb til den oprindelige ikke-tomme job-/fejl-/rollback-matrix; den observerede inventory-projektion skal afstemmes mod faktiske schedulerfences, før kode ændres.

#726s ikke-tomme schedulerforløb er nu sammensat prøvet i den eksisterende host-suite: queued/completed/unresolved historik, source/staged dispatchfences, ny ejerconsent uden blind genkørsel og en target-only bogført12,34-postering bevaret ved signeret frisk recovery/genåbning. `issue726-evidence.json` afgrænser dette fra faktiske native/hosted/J06- og releasekrav. Næste konkrete accept i samme issue er procesdød før/efter activation med kendt staged/active myndighed og målt komponentgendannelse; genbrug eksisterende testprocesmønster og allerede accepterede corrupt-/migration-/wrong-target-negativer. Vilkårlige pre-freeze kopier må ikke erklæres fenced alene på SQLite-triggerprøver.

#726s faktiske Owner-proces er nu SIGKILL-prøvet før aktivering og efter varig aktivering/før færdigsvar. Ny Owner genlæser præcis staged/active, source forbliver moved/fenced; kun sikkert staged genoptages med den oprindelige godkendelse. Alle4sammensatte host-tests består, komponentgenåbning373/366ms; dette er ikke fuld operatør-RTO eller en injektion inde i SQLite-commit. Næste originale rest er en enkelt raw pre-freeze-kopi: eksisterende staged-nonce-/inode-/ABA-prøver må ikke forveksles med kopiens almindelige åbning/skriveret.

Den eksplicitte raw pre-freeze-kopiprøve i #726 har nu påvist en reel fejl: original=moved, target=active, kopien åbner active og accepteres af transferens skrivegate. Ingen finansielle effekter; syntetisk fixture og fejlet prøve bevares. Ret nu kun den oprindelige kopibeskyttelse via vedvarende fysisk namespacebinding, fail-closed normalåbning og eksplicit autoriseret create/restore/stage/enrollment. Bevar portable business context30 adskilt fra physical control32; gamle pre-feature31-kopier med gamle31-binærer kræver fortsat den eksisterende cutovers valg af autoritativ kilde/stop/custody, ikke en falsk global tilbagekaldelsespåstand.


Den påviste kopifejl er nu rettet i `0814617f` og integreret via SGD i `f471cd8b`: fysisk control32 binder den faktisk holdte rod, rå kopier afvises ved normal åbning, og autoriseret stoppet31-enrollment bevarer historik, sessioner og adgangsevidens. De5transferprøver består; eksisterende signed recovery og original716 source-restore består. Den aktuelle server er desuden prøvet mod det bevarede syntetiske regnskab med normal eksisterende MFA-ejer:31→32 bevarer bogføringen, fælles React læser journal/serverorigin, og den bevarede gamle31-runtime afviser32. Ingen nye finansielle/providerhandlinger. Se `namespace-custody.md` for operatørhandling og den uændrede gamle31-kopi/cutovergrænse. Tidligere31-pakker kvalificerer deres gamle revisioner; de kvalificerer ikke den nye32-kandidat. Hele #726 holdes åbent for originale fulde J06-/platform-/release-/operatør-RPO/RTO-gates. Næste uafhængige oprindelige rest er #727s konkrete installations-/MCP-/agent-/supportvejledninger; faktisk retirement må vente på sine originale gates.


#727s uafhængige dokumentationspakke er nu gennemgået: kanonisk Rust-onboarding, faktisk native MCP-installation, aktuelle agentkontrakter og ejer-/operatørhjælp til backup, lokal fejl, flyttede data og et manuelt gennemgået supportrecord uden secrets/private regnskabsdata. Den tilbageværende Bun-MCP-wrapper og eksplicitte legacy-CLI er beskrevet ærligt; ingen runtime er fjernet, og ingen automatisk supportcollector/redaktionsgaranti er opfundet. `issue727-evidence.json` binder vejledningerne til aktuelle kildekontrakter og bevarer hele det oprindelige retirement-/release-/cutoverscope. Hele issuet kan først lukkes efter de oprindelige afhængigheder og kvalificeret faktisk runtimefjernelse.


Efter den nødvendige control32-rettelse er #724s matchende MacARM64-kandidat nu genbygget på ren `8e41bbc9` gennem samme builder. Manifestets13filer, samme React/icon, tre native binæridentiteter og faktisk pakkeserver control32/company36/API1 er verificeret; den eksisterende headless Managed-ejer/crash/genåbningstest består én gang mod pakkens aktuelle server. Appen er bevaret separat i `.sgd/accepted/issue724-schema32`; den gamle31pakke er fortsat historisk evidens. Ingen GUI, installation, signering eller promotion er udført. Næste nødvendige artefakt er den matchende aktuelle Linux-serverkandidat i #725, fortsat gennem eksisterende pakkeværktøjer og uden nye finansielle/providerfabrikker.


#725s nødvendige aktuelle32 LinuxARM64-kandidat er nu bygget på ren `0aba5cc2`: lokalt image `sha256:db7f093f75c49178a4f3034c92dc0046a9b36ffa345c0cfba47b6c3c2fe1939e`,39præcise filer/modes, native448MCP-katalog og faktisk control32/company36/API1. Det bevarede signerede syntetiske R17-arkiv blev uafhængigt verificeret og gendannet til et frisk, afgrænset Linuxmål uden kilde/signer: normal ny ejer/MFA/recoverylogin, præcis momsreference/bogføring/original, alle8finansielle/periodeinventarer og17oprindelige audithændelser plusrestoreboundary er bevaret efter genstart. Fysisk32binding svarer til den observerede Linuxrod før/efter genstart. Ingen nye bogføringer/indberetninger/providerhandlinger. Målcontainere/volumes er ryddet efter bevisbevaring. Alle12åbne originale issues er ved ny GitHub-triage store slutgates: installeret/claimed-platform desktop, signing, uafhængigt review, main/registry/reproducibility/promotion og fuld cutover/retirement. Brug eksisterende PR/CI til konkret uafhængig kontrol, hvis tilgængelig; genstart ikke accepterede forløb eller opfind flere små delspor.
