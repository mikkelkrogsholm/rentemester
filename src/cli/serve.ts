// `rentemester serve` — starts the cockpit backend (#170).
//
// The cockpit backend is a local JSON API over the workspace + core, consumed
// by the (separate) React cockpit app. This command only resolves config and
// hands off to `startCockpitServer`; all request logic lives in `src/server/`.
//
// `serve` is workspace-scoped (it serves an entire workspace), so it takes
// `--workspace` / `RENTEMESTER_WORKSPACE` — never the per-command `--company`.
// It is a read/workspace-management endpoint set, so it is NOT a mutating
// command in the actor-policy sense.

import { existsSync, lstatSync, readdirSync } from "node:fs";
import {
  resolveWorkspaceRoot,
  listWorkspaceCompanies,
  companyRootForSlug,
  initWorkspace,
  workspaceExists,
} from "../core/workspace";
import { openWorkspaceControlDb } from "../core/workspace-control";
import { startTaskRuntime } from "../core/task-runtime";
import { resolveServerConfig } from "../server/config";
import { startCockpitServer } from "../server/app";
import { describeStaticUiBuild } from "../server/static";
import { loadBilagsmailImapConfig } from "../core/bilagsmail";
import { createImapClient, pollImapMailbox, resolveImapConfig } from "../core/imap-intake";
import { openDb, migrate } from "../core/db";
import { companyPaths } from "../core/paths";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";

/**
 * A packaged local container owns its mounted data volume and may therefore
 * initialise a genuinely empty one on first boot. Other deployment profiles
 * must be provisioned explicitly, and no profile may adopt an arbitrary
 * non-empty directory as an accounting workspace.
 */
export function prepareWorkspaceForServe(
  workspaceRoot: string,
  deploymentProfile: "local" | "local-container" | "hosted" | undefined,
): void {
  if (!workspaceExists(workspaceRoot)) {
    if (deploymentProfile !== "local-container") {
      throw new Error("workspace is not initialized; initialize it explicitly before serving");
    }
    if (existsSync(workspaceRoot)) {
      const stat = lstatSync(workspaceRoot);
      if (!stat.isDirectory()) throw new Error("--workspace must name a directory, not a file");
      if (readdirSync(workspaceRoot).length > 0) {
        throw new Error("workspace is not initialized; refuse to adopt a non-empty directory");
      }
    }
    initWorkspace(workspaceRoot);
  }

  // Startup owns control-schema migration. `/api/ready` remains read-only and
  // can therefore prove that the exact state needed by workspace routes exists.
  openWorkspaceControlDb(workspaceRoot).close();
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("serve", null, (ctx: CommandContext) => {
    const workspaceFlag = ctx.trimToNull(ctx.arg("--workspace"));
    const hostFlag = ctx.trimToNull(ctx.arg("--host")) ?? undefined;
    const portRaw = ctx.parseOptionalNumber("--port");
    if (!portRaw.ok) return ctx.fatal(portRaw.error);

    let workspaceRoot: string | undefined;
    if (workspaceFlag) {
      try {
        workspaceRoot = resolveWorkspaceRoot(workspaceFlag);
      } catch (error) {
        return ctx.fatal(error instanceof Error ? error.message : String(error));
      }
    }

    let config: ReturnType<typeof resolveServerConfig>;
    try {
      config = resolveServerConfig({
        host: hostFlag,
        port: portRaw.value,
        workspaceRoot,
      });
    } catch (error) {
      return ctx.fatal(error instanceof Error ? error.message : String(error));
    }

    try {
      prepareWorkspaceForServe(config.workspaceRoot, config.deploymentProfile);
    } catch (error) {
      return ctx.fatal(error instanceof Error ? error.message : String(error));
    }

    const cockpit = startCockpitServer(config);
    const stopTaskRuntime = startTaskRuntime(config.workspaceRoot, { allowLocalRecipient: config.deploymentProfile !== "hosted" });

    // #349 — periodisk IMAP-polling pr. virksomhed med en gemt config.
    // Intervallet styres af --imap-poll-interval-sec (eller miljø-variablen
    // RENTEMESTER_IMAP_POLL_INTERVAL_SEC); default er 0 = slået fra. En værdi
    // > 0 starter en setInterval der pr. tick løber alle virksomheders mail-
    // bokse i workspacet. Fejl pr. virksomhed lækker IKKE op — vi logger og
    // går videre, så et nedlukket IMAP ikke vælter serven.
    const pollIntervalArg = ctx.parseOptionalNumber("--imap-poll-interval-sec");
    if (!pollIntervalArg.ok) return ctx.fatal(pollIntervalArg.error);
    const envInterval = process.env.RENTEMESTER_IMAP_POLL_INTERVAL_SEC;
    const intervalSec =
      pollIntervalArg.value ??
      (envInterval ? Number(envInterval) : 0);
    let imapTimer: NodeJS.Timeout | undefined;
    if (Number.isFinite(intervalSec) && intervalSec > 0) {
      imapTimer = setInterval(
        async () => {
          for (const c of listWorkspaceCompanies(config.workspaceRoot)) {
            const companyRoot = companyRootForSlug(
              config.workspaceRoot,
              c.slug,
            );
            const imap = loadBilagsmailImapConfig(companyRoot);
            if (!imap) continue;
            const resolved = resolveImapConfig({
              host: imap.host,
              port: imap.port,
              username: imap.username,
              password: imap.password,
              tls: imap.secure ?? true,
              mailbox: imap.mailbox ?? "INBOX",
            });
            if (!resolved.ok) continue;
            try {
              const dbPath = companyPaths(companyRoot).db;
              const db = openDb(dbPath);
              try {
                migrate(db);
                const client = createImapClient(resolved.config);
                await pollImapMailbox(db, companyRoot, client, {});
              } finally {
                db.close();
              }
            } catch (err) {
              // Best-effort — log to stderr og gå videre. En død IMAP-mail-
              // server eller en sløv virksomhedsmappe må aldrig vælte serven.
              const message = err instanceof Error ? err.message : String(err);
              process.stderr.write(
                `[imap-poll] ${c.slug}: ${message}\n`,
              );
            }
          }
        },
        intervalSec * 1000,
      );
    }

    // A clean shutdown on Ctrl-C / SIGTERM so the socket is released.
    const shutdown = () => {
      stopTaskRuntime();
      if (imapTimer) clearInterval(imapTimer);
      cockpit.stop();
      process.exit(0);
    };
    process.on("SIGINT", shutdown);
    process.on("SIGTERM", shutdown);

    // EJER-5: say WHICH cockpit-UI build is served. `app/dist` is gitignored
    // and `serve` never rebuilds it, so without this the operator has no way
    // to see that the served UI is weeks older than `app/src`.
    const uiBuild = describeStaticUiBuild(config.staticRoot);
    if (uiBuild.present) {
      process.stderr.write(
        `[serve] Cockpit-UI: ${uiBuild.staticRoot} (bygget ${uiBuild.builtAt}). ${uiBuild.rebuildHint}\n`,
      );
    } else {
      process.stderr.write(`[serve] ${uiBuild.hint}\n`);
    }

    ctx.emitResult({
      ok: true,
      message: `Cockpit backend listening on ${cockpit.url}`,
      url: cockpit.url,
      host: config.host,
      port: cockpit.server.port,
      workspace: config.workspaceRoot,
      authRequired: config.authRequired,
      deploymentProfile: config.deploymentProfile,
      ui: uiBuild,
    });
    // `Bun.serve` keeps the process alive; the command intentionally does not
    // return until the process is signalled.
  });
}
