# Runtime ownership boundary and transition obligations

The target remains one owning runtime per workspace, with local/remote clients connecting through its contracts. A runtime process lease is necessary; it is not sufficient to authorize migration, company access or a ledger commit.

Source investigation at base 0e6c9aa1 found no runtime lease. Company openDb (src/core/db.ts:245) configures WAL and busy_timeout; those serialize database activity, not runtime ownership. Workspace control opens/migrates separately (workspace-control.ts:559); hosted Better Auth retains a writable control connection (server/better-auth.ts). Direct migration/backup connections exist in cli/system.ts and core/system-backups.ts. Restore stages and promotes directories, initialization creates directories before DB open, and workspace manifests/backup keys/configuration have file-only writes. A guard only in openDb would miss these effects and every already-open handle.

CLI and MCP also accept arbitrary company paths independently of a configured workspace. The current direct-child check is lexical. Locking an environment-selected workspace while mutating another raw company path would protect the wrong scope. Future admission must bind physical and stable logical identity and handle standalone companies/restore destinations explicitly.

Use a kernel flock on the existing root directory for this primitive. This avoids creating a replaceable sentinel and avoids mixing ownership with SQLite database locking or control-database writes. Ordinary filesystem open/close on a SQLite sentinel can interfere with SQLite POSIX locking, and unlink/recreate can split inode ownership; see [SQLite's locking hazards](https://www.sqlite.org/howtocorrupt.html). SQLite WAL also assumes same-host shared-memory coordination; see [WAL restrictions](https://www.sqlite.org/wal.html). Directory flock remains cooperative and platform/filesystem-dependent.

Keep these remaining gates explicit:

1. Bind each writable runtime to one enrolled logical workspace and physical root; pass owned capabilities rather than environment strings into mutating APIs.
2. Admit every legacy CLI/MCP/HTTP/job/config/backup/restore path before its first effect, retaining the lease until writes/jobs and handles drain. Resolve mixed/multi-workspace scope before acquiring in a fixed order.
3. Stop unpatched legacy runtimes and jobs through the existing release/cutover process. No PID/timeout/metadata heuristic can prove that an arbitrary old process has ceased writing; automatic live adoption is forbidden.
4. Define authenticated IPC discovery/client reconnect and busy behavior. Unreachable owner never authorizes direct-write fallback. No nonce text or metadata file alone grants authority.
5. Qualify local unsynchronized storage and each supported OS, plus clone/restore lineage and destination protection. Physical inode identity is not global workspace identity.
6. Port transactional migrations, sequence allocation and cross-store authority fencing. A process lease alone cannot provide atomic DB-plus-file effects or revocation ordering.

This increment changes no legacy product entrypoint and enables no Rust writer. The development example and cooperative cross-language test establish the primitive, not the full transition gate. R03/R04/R12/R13 and the full epic remain open.
