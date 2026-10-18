# Private broker connection owner

The `rentemester-host` crate composes the existing namespace guard with privately owned control and company SQLite connections. This increment is the physical owning runtime for an offline-provisioned Linux broker, not a production writer, schema adoption, fresh initializer, bootstrap, transport, or deployment qualification. The existing general product and all 55 epic issues remain the goal; local macOS and remote desktop modes remain required.

## Trusted boundary and construction

`Owner::open_protected(root, broker_uid, workspace, slug)` first acquires the dedicated-UID `NamespaceGuard`, including the workspace lease and pinned layout. Only then does it open the two named files with READ_WRITE and NOFOLLOW, without CREATE or URI flags. It configures foreign keys and FULL synchronous mode and verifies actual handles against the retained layout. It does not choose new journal modes or expose a connection supplied by a caller.

The launcher's strict Linux profile, root/operator and all enrolled broker-UID code remain trusted. Separate client/legacy UIDs cannot obtain new file access. Provisioning must be offline and protect files from creation; the guard cannot revoke prior, inherited or passed descriptors. Current metadata does not establish absence of earlier clients. This constructor alone does not prove native schema, logical lineage, prior-writer drain, authentication bootstrap, or permission to adopt existing data. SQLite may perform ordinary hot-journal/WAL recovery when opening/configuring existing files; this is a possible persistent effect even without an explicit repair command.

Every partially constructed owner already holds its connections and namespace in one private resource set. A later open, configuration or identity error drops that set through the same explicit-close mechanism as shutdown. No CREATE, chmod, chown, unlink, namespace repair or migration is performed by this crate.

## Fixed dispatch and outcomes

Four public methods directly invoke the frozen application use cases: `post_human`, `reverse_human`, `post_service`, `reverse_service`. The selected workspace/slug come from the owned layout binding and revalidated manifest; callers cannot claim a different registration/archive status. The application retains its actual authorization, session/key, role, idempotency and transaction policy. Trusted adapters still own credential verification and transport-specific gates; these methods are not HTTP endpoints.

There is no public generic callback, mutable connection reference, raw handle, connection-bearing failure, conversion, Deref or AsMut. A generic callback could move out a connection with safe `mem::replace`; even a callback returning unit could capture it. Such an API would invalidate lifetime ownership and is deliberately absent.

Admission marks the owner retired *before* revalidation and dispatch. An error or unwinding panic cannot silently restore it. Only an application result whose connection reuse flags allow it, followed by successful physical revalidation, makes the owner reusable. `Attempt` preserves the exact original application result and separately reports retirement. A committed/unknown outcome must not turn into a fresh retry merely because the owner retires afterward. No operation is admitted on a retired owner.

## Confirmed closure and quarantine

`try_close(self)` retires the owner and calls consuming `Connection::close` on both slots. Every successful close removes that slot; an `Err((connection, error))` restores the exact still-open connection. The guard, all pinned descriptors and workspace lease remain until **every** connection is confirmed closed. Other successfully closed slots need not remain open.

A `CloseFailure` privately owns a boxed retired owner. Its sole operation is `retry`; it does not expose the owner or connections. Dropping an owner or close failure retries explicit closure. If any close still fails, it forgets the entire unresolved resource set, intentionally quarantining the handles, pins and lease until process exit. It never finalizes unowned statements or relies on ordinary `Connection::Drop`, which discards sqlite3_close failure. Process termination lets the OS release those resources. Quarantine is a last-resort safety behavior, not a recovery or availability guarantee.

| Path | Required result |
| --- | --- |
| Both closes succeed | Release layout and lease after both closes |
| Real outstanding statement makes either close BUSY | Keep exact handle, layout and lease; failure is retired |
| Retry still BUSY | Keep protection unchanged |
| Ordinary Drop with BUSY | Quarantine entire unresolved set until process exit |
| Partial construction errors/panics after first connection | Same close/quarantine behavior |
| Dispatch unwinds or physical identity changes | Retired permanently; locks remain until close/process exit |

The implementation contains no ordinary regular-file reopening/copy/hash operations while owned handles are live. The application and code compiled in the owner module remain trusted. This is API encapsulation, not a sandbox for malicious same-UID code.

## Evidence and remaining acceptance

`readiness.json` is scoped structural GDD readiness, not behavioral proof. The model was written during implementation, not a claim of pre-implementation validation. Native tests use real persistent files and child-process lock contenders. Linux qualification uses the actual `Owner` under broker UID 10001 and clients under 10002 in an isolated pinned-image Docker filesystem, without mounting real workspaces. Test-only fault hooks are excluded from the production build.

The qualification covers normal commit/confirmed close/persistent reopen in WAL and DELETE, actual close BUSY on each database, retry, Drop quarantine, process-exit release, partial-constructor clean/error/panic paths, sticky namespace invalidation/restoration and dispatch-admission panic. Tests distinguish a closed other database from the still-busy database while the whole workspace lease remains held. Independent compile-fail consumers check the absence of connection extraction APIs.

See `evidence.json`, `parity.json`, `native.txt`, `clippy.txt` and `review.json` for measured results and exact source hashes. Fixed typed application dispatch is composed and compiles; full successful canonical-schema posting/reversal through this owner, actual server/CLI/MCP composition, initializer/bootstrap, crash/power-loss deployment qualification, actual legacy CLI under client UID, macOS namespace qualification and all UI/distribution/cutover acceptance remain open. Prior full integration results are identified as prior evidence and are not silently presented as a new run.
