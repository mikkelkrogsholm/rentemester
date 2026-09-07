import { migrate } from "../core/db";
import { closeAccountingPeriod, reopenAccountingPeriod } from "../core/periods";
import { computePeriodCloseReadiness, loadPeriodCloseReview, periodCloseReviewSchemaAvailable, projectHumanReadiness, reviewPeriodCloseReadiness } from "../core/period-close-readiness";
import { openCommandDb } from "../cli-dispatch";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import { companyPaths } from "../core/paths";
import { inspectOpenLedger, openLedgerReadOnly } from "../core/ledger-inspection";
import {
  actorMatchesAllowlist,
  inferredMutationActor,
  isCanonicalActorId,
  loadActorAllowlist,
  trimToNull,
} from "../cli-actor";

function confirmed(ctx: CommandContext): void { if (ctx.arg("--confirm") !== "yes") ctx.fatal("--confirm must be exactly yes"); }
function withReadOnlyCurrentLedger(ctx: CommandContext, action: (db: ReturnType<typeof openLedgerReadOnly>) => void): void {
  const db=openLedgerReadOnly(companyPaths(ctx.companyRoot()).db);
  try {
    const schema=inspectOpenLedger(db);
    if(schema.status==="corrupt" || schema.status==="newer") { ctx.emitResult({ok:false,errors:["PERIOD_CLOSE_READ_UNAVAILABLE"],schema}); return; }
    action(db);
  } finally { db.close(); }
}

/**
 * `period reopen` is a controlled, fully audit-logged mutation (#247), but it
 * is not registered in the central `MUTATING_COMMANDS` actor gate. It must
 * still be clearly attributable, so the actor policy is enforced here, in the
 * handler, mirroring `enforceMutationActorPolicy`: an explicit `--actor` must
 * be canonical and in `config/policy.yaml`; otherwise an inferred actor
 * (USER/LOGNAME/USERNAME/OPENCLAW_AGENT) must exist. The resolved actor is exported via
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
      "actor required for mutations: pass --actor <user:...|agent:...|system:...> or run with USER/LOGNAME/USERNAME/OPENCLAW_AGENT set",
    );
  }
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("period", "readiness", (ctx) => {
    const from = ctx.arg("--from"); const to = ctx.arg("--to");
    if (!from || !to) { console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>"); process.exit(2); }
    withReadOnlyCurrentLedger(ctx,db=>{ const packet=computePeriodCloseReadiness(db, { periodStart: from, periodEnd: to, companyRoot: ctx.companyRoot() }); ctx.emitResult({ok:true,packet,readiness:projectHumanReadiness(packet)}); });
  });
  dispatch.on("period", "review", (ctx) => {
    const from = ctx.arg("--from"); const to = ctx.arg("--to");
    if (!from || !to) { console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>"); process.exit(2); }
    const expectedPacketHash = ctx.arg("--packet-hash");
    if (!expectedPacketHash || !/^[a-f0-9]{64}$/i.test(expectedPacketHash)) {
      console.error("Missing required --packet-hash <sha256>"); process.exit(2);
    }
    confirmed(ctx);
    const actor = ctx.cliActor ?? process.env.RENTEMESTER_ACTOR;
    if (!actor) { console.error("actor required for mutations"); process.exit(2); }
    const db = openCommandDb(ctx); migrate(db);
    const packet = computePeriodCloseReadiness(db, { periodStart: from, periodEnd: to, companyRoot: ctx.companyRoot() });
    if (packet.hash !== expectedPacketHash) {
      ctx.emitResult({ ok: false, errors: ["PERIOD_CLOSE_PACKET_STALE_OR_MISSING"], packetHash: packet.hash });
      db.close();
      process.exit(1);
    }
    ctx.emitResult(reviewPeriodCloseReadiness(db, { packet, reviewerActor: actor, reviewerPrincipal: { kind: "local-trusted", subjectId: actor } }) as unknown as Record<string, unknown>);
    db.close();
  });
  dispatch.on("period", "status", (ctx) => {
    const reviewId = Number(ctx.arg("--review-id"));
    if (!Number.isSafeInteger(reviewId) || reviewId < 1) { console.error("Missing required --review-id <positive integer>"); process.exit(2); }
    withReadOnlyCurrentLedger(ctx,db=>ctx.emitResult(periodCloseReviewSchemaAvailable(db)?{ok:true,review:loadPeriodCloseReview(db, reviewId)}:{ok:true,status:"unavailable",code:"PERIOD_CLOSE_REVIEW_SCHEMA_UNAVAILABLE",review:null}));
  });
  dispatch.on("period", "close", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const force = ctx.arg("--force") === "yes" || ctx.arg("--force") === "true";
    confirmed(ctx);
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
      force,
      readinessPacketHash: ctx.arg("--packet-hash") ?? undefined,
      readinessReviewId: Number(ctx.arg("--review-id")) || undefined,
      forceReason: ctx.arg("--reason") ?? undefined,
      createdBy: ctx.cliActor ?? process.env.RENTEMESTER_ACTOR,
      companyRoot: ctx.companyRoot(),
      // Local actor attribution is deliberately not an authorization grant.
      // CLI force therefore fails closed unless a future trusted local
      // authorization provider is introduced; hosted requests use live RBAC.
      forceConfirmed: true,
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
