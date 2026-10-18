# Exclusive fresh native workspace initialization

`Owner::create_new` builds a new protected Linux workspace and returns its physically owning runtime plus native schema/ledger identity observations. This increment advances the persistent Rust server path. First-user/auth bootstrap, attested native reopen lineage, existing-data adoption/drain, transport composition, macOS local namespace support, UI/distribution/cutover and the complete epic remain required.

## Inputs and boundary

A trusted offline launcher supplies absolute new root, dedicated broker UID, workspace label, company slug, display name and creation metadata. Runtime validates its current-thread profile, manifest selection/metadata and every parent ancestor including absence of default/access ACLs before mkdir. The existing profile requires broker/root ownership, no group/world-write, no symlinks, supported ACL observation, NoNewPrivs, umask 0077, matching UID 4 and zero capabilities. Broker UID and root/operator code remain trusted/enrolled. No previous, inherited or passed client descriptors may exist; protection must apply from creation.

Existing directories, regular files and symlinks all return `workspace_exists`. Even an empty existing directory is not reused. There is no CREATE IF NOT EXISTS, adoption, repair, chmod/chown, recursive deletion or best-effort resume of a failed creation. Invalid input or unsafe parent/process refuses before the new directory is created. A supported process can still fail after mkdir, for example on an unsupported filesystem or I/O error; the new private partial tree is retained for inspection and a later create refuses it.

## Namespace before SQLite

`NamespaceGuard::create_fresh` uses mkdirat with exclusive creation semantics relative to the checked parent and 0700 mode. It validates the resulting ancestry and acquires the root lease before creating internal directories and regular files. Files use CREATE|EXCL|NOFOLLOW,0600 and explicit fsync; directories and parent are synced. No actual existing workspace is opened or modified by this operation. Parent and new root identities are compared, and the held lease's identity must match the checked directory.

The manifest and two zero-length database files are created before SQLite is opened. Every temporary regular creation descriptor closes before binding the retained layout and opening SQLite. The original root lease is transferred directly into `WorkspaceLayout`; there is no release/reacquire interval. Existing `NamespaceGuard::acquire` also verifies lease versus directory identity through the shared binding path. All subsequent live observations retain the existing no-regular-reopen discipline.

Host `open_guarded` installs the complete private resource set before the first READ_WRITE|NOFOLLOW SQLite open, using the existing confirmed-close/BUSY/quarantine rules for all failures. `create_new` retires admission while building schemas and only makes the owner reusable after initialization and actual namespace/handle revalidation succeed. A constructor error or panic can never expose the partial owner or its connections.

## Canonical schema construction

`fresh::initialize_empty_pair` operates solely on caller-owned connections. The caller must prove fresh-file provenance and exclusive physical ownership. Both databases preflight before persistent SQL: writable, autocommit, no main or temp objects, no attached databases, zero user_version/application_id and query_only off. Company metadata must be valid. Empty schema alone is not proof that an older file was never used; the host supplies the proof by exclusive new creation.

The control builder validates all 30 embedded artifacts against the existing checksum catalogue and applies DDL plus exact history in one IMMEDIATE transaction. The company builder validates the normalized baseline image and 35 later artifacts, builds/stamps all 36 versions, applies current final guards, inserts the caller's company name and verifies the complete canonical schema, migration history, foreign keys and required singletons before commit. New ledger UUID generation is the existing immutable 128-bit mechanism. No private-copy rehearsal report or migration/adoption entrypoint is substituted for this live initializer.

The two database commits are independent, not atomic together. If company initialization fails after control commits, control can be complete while company remains empty. No initialized report/owner is returned; the whole owning constructor closes safely or quarantines unresolved handles, and the private partial tree remains. No retry/adoption capability is granted. Tests exercise real company write contention and check that the completed control and untouched company are observable accurately.

The report contains versions 30/36, full schema fingerprints and the ledger UUID; it grants no identity adoption or authorization. It creates the company profile with its supplied name and existing general schema defaults. It does not seed an account plan, balances, users, credentials, sessions, memberships, bootstrap reservation or access rights. Concrete company data belongs in the new workspace, never in product defaults or checked-in tests.

## Evidence and remaining work

Native tests check complete schemas and empty authority tables, distinct ledger identities across synthetic companies, persistent reopen, repeat/nonempty rejection before writes, transaction/temp/attachment/metadata refusal and partial two-database failure. The isolated Linux driver additionally exercises actual new namespace creation under broker UID 10001, different-UID denials, lease continuity, existing/unsafe path refusal, default ACL read-back, failed-creation BUSY quarantine and concurrent exclusive creators. It reruns the existing 50 physical-owner checks. Only synthetic files are created in task-owned Docker volumes; no real workspace is mounted.

`readiness.json` is structural GDD readiness authored during implementation. Measured results, exact source hashes and independent review live in `evidence.json`, `parity.json`, `native.txt`, `clippy.txt` and `review.json`. Test-only constructor faults are absent from production builds. Physical reopen of a just-created synthetic pair demonstrates persistence, not native lineage/adoption authority for arbitrary existing files. First-user bootstrap and successful authenticated canonical-domain posting through the owner remain the next composition steps. Container ext-family fsync execution is not a disk power-loss/deployment qualification.
