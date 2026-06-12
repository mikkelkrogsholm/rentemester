import { migrate } from "../core/db";
import { closeAccountingPeriod, reopenAccountingPeriod } from "../core/periods";
import { openCommandDb } from "../cli-dispatch";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import {
  actorMatchesAllowlist,
  inferredMutationActor,
  isCanonicalActorId,
  loadActorAllowlist,
  trimToNull,
} from "../cli-actor";

/**
 * `period reopen` is a controlled, fully audit-logged mutation (#247), but it
 * is not registered in the central `MUTATING_COMMANDS` actor gate. It must
 * still be clearly attributable, so the actor policy is enforced here, in the
 * handler, mirroring `enforceMutationActorPolicy`: an explicit `--actor` must
 * be canonical and in `config/policy.yaml`; otherwise an inferred actor
 * (USER/LOGNAME/OPENCLAW_AGENT) must exist. The resolved actor is exported via
 * `RENTEMESTER_ACTOR` so the core audit log attributes the reopen correctly.
 */
function enforceReopenActor(ctx: CommandContext, root: string): void {
  const explicitActor = ctx.cliActor ?? trimToNull(process.env.RENTEMESTER_ACTOR);
  if (explicitActor) {
    if (!isCanonicalActorId(explicitActor)) {
      ctx.fatal("explicit actor must use canonical format user:<id>, agent:<id>, or system:<id>");
    }
    const allowlist = loadActorAllowlist(root);
    // #248: case-insensitive match — an explicit `--actor user:mikkel` and a
    // derived USER=Mikkel are the same identity; the allowlist must not reject
    // one form while letting the other through.
    if (!actorMatchesAllowlist(explicitActor, allowlist)) {
      ctx.fatal(
        `actor '${explicitActor}' is not in config/policy.yaml actor_allowlist; add it or run without --actor`,
      );
    }
    process.env.RENTEMESTER_ACTOR = explicitActor;
    if (ctx.cliActorVia) process.env.RENTEMESTER_ACTOR_VIA = ctx.cliActorVia;
    else if (!trimToNull(process.env.RENTEMESTER_ACTOR_VIA))
      process.env.RENTEMESTER_ACTOR_VIA = "rentemester-cli";
    return;
  }
  if (!inferredMutationActor()) {
    ctx.fatal(
      "actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/OPENCLAW_AGENT set",
    );
  }
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("period", "close", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = closeAccountingPeriod(db, {
      periodStart: from,
      periodEnd: to,
      kind: (ctx.arg("--kind") as any) ?? undefined,
      status: (ctx.arg("--status") as any) ?? undefined,
      reference: ctx.arg("--reference") ?? undefined,
      // Bypass the open-high/medium-exceptions safety guard (Batch D-7).
      // The bypass itself is visible in the close result + audit log.
      force: ctx.arg("--force") === "yes" || ctx.arg("--force") === "true",
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("period", "reopen", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    const reason = ctx.arg("--reason");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    if (!reason || !reason.trim()) {
      console.error("Missing required --reason <text>");
      process.exit(2);
    }
    // A reopen must be attributable — enforce the actor before mutating.
    enforceReopenActor(ctx, ctx.companyRoot());
    const db = openCommandDb(ctx);
    migrate(db);
    const result = reopenAccountingPeriod(db, {
      periodStart: from,
      periodEnd: to,
      kind: (ctx.arg("--kind") as any) ?? undefined,
      reason,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });
}
