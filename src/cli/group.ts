import { readFileSync } from "node:fs";
import { resolveWorkspaceRoot, listWorkspaceCompanies, companyRootForSlug } from "../core/workspace";
import { openWorkspaceControlDb, openWorkspaceControlReadOnlyDb } from "../core/workspace-control";
import { applyGroupManifest, getGroupStructureOverview, GROUP_CONSOLIDATION_BLOCKERS, parseGroupManifest, readCurrentGroupManifest, type GroupManifest } from "../core/group-manifest";
import { approveIntercompanyMapping, buildIntercompanyReconciliation, parseIntercompanyMapping, proposeIntercompanyMapping, readIntercompanyMappingState, revokeIntercompanyMapping, type IntercompanyMapping } from "../core/intercompany-reconciliation";
import { applyBalanceElimination, approveBalanceElimination, proposeBalanceElimination, readAppliedBalanceEliminations, readBalanceEliminationState, rejectBalanceElimination, reverseBalanceElimination } from "../core/consolidation-eliminations";
import { approveConsolidationProfile, buildConsolidatedReport, parseConsolidationProfile, proposeConsolidationProfile, readConsolidationProfileState, revokeConsolidationProfile, type ConsolidationProfile } from "../core/consolidated-reports";
import { approveIntercompanyDisposition, inspectIntercompanyDisposition, intercompanyDispositionStatus, linkIntercompanyDispositionJournal, planIntercompanyDisposition, proposeIntercompanyDisposition, reopenIntercompanyDisposition, settleIntercompanyDisposition, supersedeIntercompanyDisposition } from "../core/intercompany-dispositions";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import { loadActorAllowlist, requireMutationActorIdentity } from "../cli-actor";

function required(ctx: CommandContext, flag: string): string {
  const value = ctx.trimToNull(ctx.arg(flag));
  if (!value) ctx.fatal(`${flag} is required`);
  return value!;
}
function readManifest(path: string): unknown {
  try { return JSON.parse(readFileSync(path, "utf8")); }
  catch { throw new Error("group manifest file must be readable valid JSON"); }
}
function actorInput(ctx: CommandContext): { createdBy: string; createdByProgram: string } {
  const createdBy = ctx.cliActor?.trim() || process.env.RENTEMESTER_ACTOR?.trim() || ctx.inferredMutationActor()?.trim();
  if (!createdBy) throw new Error("actor is required");
  return { createdBy, createdByProgram: ctx.cliActorVia?.trim() || process.env.RENTEMESTER_ACTOR_VIA?.trim() || "rentemester-cli" };
}
function dispositionActorInput(ctx: CommandContext): ReturnType<typeof actorInput> {
  requireMutationActorIdentity(ctx.cliActor, ctx.fatal);
  return actorInput(ctx);
}

/** A group manifest changes workspace-wide legal-entity metadata.  The actor
 * must therefore be explicitly allowed by every non-archived entity it names;
 * a convenient unrelated policy company must never substitute for that. */
function assertEveryReferencedCompanyAuthorizes(workspace: string, manifest: ReturnType<typeof parseGroupManifest>, actor: string): void {
  const active = new Map(listWorkspaceCompanies(workspace).filter((company) => !company.archived).map((company) => [company.slug, company]));
  const referenced = new Set(manifest.groups.flatMap((group) => [
    ...group.memberships.map((membership) => membership.companySlug),
    ...group.ownership.flatMap((edge) => [edge.parentCompanySlug, edge.childCompanySlug]),
  ]));
  for (const slug of referenced) {
    if (!active.has(slug)) continue;
    const allowlist = loadActorAllowlist(companyRootForSlug(workspace, slug));
    if (allowlist.size === 0 || ![...allowlist].some((entry) => entry.trim().toLowerCase() === actor.trim().toLowerCase())) {
      throw new Error(`actor is not explicitly authorised by active company '${slug}' for group apply-manifest`);
    }
  }
}

function assertMappingCompaniesAuthorize(workspace: string, mapping: IntercompanyMapping, actor: string, operation: string): void {
  assertCompanySlugsAuthorize(workspace, [mapping.leftCompanySlug, mapping.rightCompanySlug], actor, operation);
}

function assertCompanySlugsAuthorize(workspace: string, slugs: readonly string[], actor: string, operation: string): void {
  const active = new Map(listWorkspaceCompanies(workspace).filter((company) => !company.archived).map((company) => [company.slug, company]));
  for (const slug of new Set(slugs)) {
    if (!active.has(slug)) throw new Error(`active mapped company '${slug}' is unavailable`);
    const allowlist = loadActorAllowlist(companyRootForSlug(workspace, slug));
    if (allowlist.size === 0 || ![...allowlist].some((entry) => entry.trim().toLowerCase() === actor.trim().toLowerCase())) {
      throw new Error(`actor is not explicitly authorised by active company '${slug}' for ${operation}`);
    }
  }
}

function profileCompanySlugs(profile: ConsolidationProfile, manifest: GroupManifest): string[] {
  const group = manifest.groups.find((candidate) => candidate.id === profile.groupId);
  if (!group) throw new Error("consolidation profile references an unknown group");
  return [...new Set(group.memberships.map((membership) => membership.companySlug))];
}

export function register(dispatch: CommandDispatch): void {
  // Dispositions are workspace evidence only.  These commands deliberately do
  // not call a company posting command or accept an elimination amount.
  dispatch.on("group", "propose-disposition", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to propose intercompany disposition"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const audit = dispositionActorInput(ctx);
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      let planned: ReturnType<typeof planIntercompanyDisposition>;
      try { planned = planIntercompanyDisposition(reader, readManifest(required(ctx, "--disposition"))); }
      finally { reader.close(); }
      assertCompanySlugsAuthorize(workspace, [planned.disposition.left.companySlug, planned.disposition.right.companySlug], audit.createdBy, "group propose-disposition");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...proposeIntercompanyDisposition(db, planned.disposition, { actor: audit.createdBy, principal: { kind: "user", id: audit.createdBy } }) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "disposition proposal failed"] }); }
  });
  dispatch.on("group", "plan-disposition", (ctx) => { try { const db=openWorkspaceControlReadOnlyDb(resolveWorkspaceRoot(required(ctx,"--workspace"))); try { ctx.emitResult(planIntercompanyDisposition(db,readManifest(required(ctx,"--disposition")))); } finally {db.close();} } catch(error) {ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition dry-run failed"]});} });
  dispatch.on("group", "approve-disposition", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to approve intercompany disposition"] }); return; }
    try { const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")), audit=actorInput(ctx), id=required(ctx,"--disposition-id"), hash=required(ctx,"--payload-hash"); const reader=openWorkspaceControlReadOnlyDb(workspace); const before=inspectIntercompanyDisposition(reader,id); reader.close(); if(!before)throw new Error("disposition not found"); assertCompanySlugsAuthorize(workspace,[before.disposition.left.companySlug,before.disposition.right.companySlug],audit.createdBy,"group approve-disposition"); const db=openWorkspaceControlDb(workspace);try{ctx.emitResult({ok:true,...approveIntercompanyDisposition(db,id,hash,{actor:audit.createdBy,principal:{kind:"user",id:audit.createdBy}})});}finally{db.close();} }catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition approval failed"]});}
  });
  dispatch.on("group", "link-disposition", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to link intercompany disposition journal"] }); return; }
    try {const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")),audit=actorInput(ctx),id=required(ctx,"--disposition-id"),side=required(ctx,"--side");if(side!=="left"&&side!=="right")throw new Error("--side must be left or right");const reader=openWorkspaceControlReadOnlyDb(workspace),before=inspectIntercompanyDisposition(reader,id);reader.close();if(!before)throw new Error("disposition not found");assertCompanySlugsAuthorize(workspace,[before.disposition[side].companySlug],audit.createdBy,"group link-disposition");const db=openWorkspaceControlDb(workspace);try{ctx.emitResult({ok:true,...linkIntercompanyDispositionJournal(db,workspace,{dispositionId:id,payloadHash:required(ctx,"--payload-hash"),side,journalEntryId:Number(required(ctx,"--journal-entry-id")),expectedLedgerHeadHash:ctx.arg("--ledger-head-hash")??null,actor:audit.createdBy,principal:{kind:"user",id:audit.createdBy}})});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"journal link failed"]});}
  });
  dispatch.on("group", "disposition-status", (ctx) => {try{const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")),db=openWorkspaceControlReadOnlyDb(workspace);try{const disposition=intercompanyDispositionStatus(db,workspace,required(ctx,"--disposition-id"),ctx.arg("--as-of")??undefined);ctx.emitResult(disposition?{ok:true,...disposition}:{ok:false,errors:["disposition not found"]});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition status unavailable"]});}});
  dispatch.on("group", "settle-disposition", (ctx) => {if (ctx.arg("--confirm")!=="yes"){ctx.emitResult({ok:false,errors:["--confirm yes required to settle intercompany disposition"]});return;}try{const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")),audit=dispositionActorInput(ctx),id=required(ctx,"--disposition-id"),reader=openWorkspaceControlReadOnlyDb(workspace),before=inspectIntercompanyDisposition(reader,id);reader.close();if(!before)throw new Error("disposition not found");assertCompanySlugsAuthorize(workspace,[before.disposition.left.companySlug,before.disposition.right.companySlug],audit.createdBy,"group settle-disposition");const evidence=JSON.parse(required(ctx,"--settlement-evidence-json"));const db=openWorkspaceControlDb(workspace);try{ctx.emitResult({ok:true,...settleIntercompanyDisposition(db,workspace,{dispositionId:id,payloadHash:required(ctx,"--payload-hash"),settlementEvidenceRecordIds:evidence,actor:audit.createdBy,principal:{kind:"user",id:audit.createdBy}})});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition settlement failed"]});}});
  dispatch.on("group", "reopen-disposition", (ctx) => {if (ctx.arg("--confirm")!=="yes"){ctx.emitResult({ok:false,errors:["--confirm yes required to reopen intercompany disposition"]});return;}try{const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")),audit=dispositionActorInput(ctx),id=required(ctx,"--disposition-id"),reader=openWorkspaceControlReadOnlyDb(workspace),before=inspectIntercompanyDisposition(reader,id);reader.close();if(!before)throw new Error("disposition not found");assertCompanySlugsAuthorize(workspace,[before.disposition.left.companySlug,before.disposition.right.companySlug],audit.createdBy,"group reopen-disposition");const db=openWorkspaceControlDb(workspace);try{ctx.emitResult({ok:true,...reopenIntercompanyDisposition(db,workspace,{dispositionId:id,payloadHash:required(ctx,"--payload-hash"),reason:required(ctx,"--reason"),actor:audit.createdBy,principal:{kind:"user",id:audit.createdBy}})});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition reopen failed"]});}});
  dispatch.on("group", "supersede-disposition", (ctx) => {if (ctx.arg("--confirm")!=="yes"){ctx.emitResult({ok:false,errors:["--confirm yes required to supersede intercompany disposition"]});return;}try{const workspace=resolveWorkspaceRoot(required(ctx,"--workspace")),audit=dispositionActorInput(ctx),id=required(ctx,"--disposition-id"),reader=openWorkspaceControlReadOnlyDb(workspace),before=inspectIntercompanyDisposition(reader,id);reader.close();if(!before)throw new Error("disposition not found");assertCompanySlugsAuthorize(workspace,[before.disposition.left.companySlug,before.disposition.right.companySlug],audit.createdBy,"group supersede-disposition");const db=openWorkspaceControlDb(workspace);try{ctx.emitResult({ok:true,...supersedeIntercompanyDisposition(db,{dispositionId:id,payloadHash:required(ctx,"--payload-hash"),replacementDispositionId:required(ctx,"--replacement-disposition-id"),reason:required(ctx,"--reason"),actor:audit.createdBy,principal:{kind:"user",id:audit.createdBy}})});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[error instanceof Error?error.message:"disposition supersession failed"]});}});
  dispatch.on("group", "validate-manifest", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const manifest = parseGroupManifest(readManifest(required(ctx, "--manifest")), listWorkspaceCompanies(workspace).map((company) => company.slug));
      ctx.emitResult({ ok: true, scope: "structure-status-only", consolidationStatus: "not-available", consolidatedFigures: null, rawCompanySums: null, blockers: GROUP_CONSOLIDATION_BLOCKERS, groupCount: manifest.groups.length });
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "group manifest validation failed"] }); }
  });
  dispatch.on("group", "apply-manifest", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to apply group structure"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      // Validate before opening the control writer: malformed input must not
      // create/migrate even the private control DB.
      const input = readManifest(required(ctx, "--manifest"));
      const manifest = parseGroupManifest(input, listWorkspaceCompanies(workspace).map((company) => company.slug));
      assertEveryReferencedCompanyAuthorizes(workspace, manifest, actorInput(ctx).createdBy);
      const db = openWorkspaceControlDb(workspace);
      try {
        const result = applyGroupManifest(db, workspace, input, actorInput(ctx));
        ctx.emitResult({ ok: true, scope: "structure-status-only", consolidationStatus: "not-available", consolidatedFigures: null, rawCompanySums: null, blockers: GROUP_CONSOLIDATION_BLOCKERS, ...result });
      } finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "group manifest could not be applied"] }); }
  });
  dispatch.on("group", "overview", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const asOf = required(ctx, "--as-of");
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try {
        // CLI overview is an explicit local operator operation. Hosted users
        // use the membership-filtered HTTP route instead.
        ctx.emitResult({ ok: true, ...getGroupStructureOverview(db, workspace, new Set(listWorkspaceCompanies(workspace).map((company) => company.slug)), asOf) });
      } finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "group overview is unavailable"] }); }
  });
  dispatch.on("group", "validate-mapping", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try {
        const current = readCurrentGroupManifest(db, workspace);
        if (!current) throw new Error("group structure must be configured before intercompany mappings");
        const mapping = parseIntercompanyMapping(readManifest(required(ctx, "--mapping")), current.manifest);
        ctx.emitResult({ ok: true, mapping });
      } finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "intercompany mapping validation failed"] }); }
  });
  dispatch.on("group", "propose-mapping", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to propose intercompany mapping"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const input = readManifest(required(ctx, "--mapping"));
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      let mapping: IntercompanyMapping;
      try {
        const current = readCurrentGroupManifest(reader, workspace);
        if (!current) throw new Error("group structure must be configured before intercompany mappings");
        mapping = parseIntercompanyMapping(input, current.manifest);
      } finally { reader.close(); }
      const actor = actorInput(ctx);
      assertMappingCompaniesAuthorize(workspace, mapping, actor.createdBy, "group propose-mapping");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...proposeIntercompanyMapping(db, workspace, input, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "intercompany mapping proposal failed"] }); }
  });
  dispatch.on("group", "approve-mapping", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to approve intercompany mapping"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const mappingId = required(ctx, "--mapping-id");
      const mappingHash = required(ctx, "--mapping-hash");
      if (!/^[0-9a-f]{64}$/.test(mappingHash)) throw new Error("--mapping-hash must be a lowercase SHA-256 digest");
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      const state = (() => { try { return readIntercompanyMappingState(reader, workspace, mappingId); } finally { reader.close(); } })();
      if (!state || state.status !== "proposed" || state.mappingHash !== mappingHash) throw new Error("exact pending intercompany mapping proposal was not found");
      const actor = actorInput(ctx);
      assertMappingCompaniesAuthorize(workspace, state.mapping, actor.createdBy, "group approve-mapping");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...approveIntercompanyMapping(db, workspace, mappingId, mappingHash, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "intercompany mapping approval failed"] }); }
  });
  dispatch.on("group", "revoke-mapping", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to revoke intercompany mapping"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const mappingId = required(ctx, "--mapping-id");
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      const state = (() => { try { return readIntercompanyMappingState(reader, workspace, mappingId); } finally { reader.close(); } })();
      if (!state || state.status !== "approved") throw new Error("approved intercompany mapping was not found");
      const actor = actorInput(ctx);
      assertMappingCompaniesAuthorize(workspace, state.mapping, actor.createdBy, "group revoke-mapping");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...revokeIntercompanyMapping(db, workspace, mappingId, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "intercompany mapping revocation failed"] }); }
  });
  dispatch.on("group", "reconcile", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try { ctx.emitResult({ ok: true, ...buildIntercompanyReconciliation(db, workspace, new Set(listWorkspaceCompanies(workspace).filter((company) => !company.archived).map((company) => company.slug)), required(ctx, "--as-of")) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "intercompany reconciliation failed"] }); }
  });
  dispatch.on("group", "propose-elimination", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to propose consolidation elimination"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const input = readManifest(required(ctx, "--elimination")) as { id?: unknown; mappingId?: unknown; asOf?: unknown; evidenceRefs?: unknown };
      if (!input || typeof input !== "object" || typeof input.mappingId !== "string") throw new Error("elimination file must contain mappingId");
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      const mapping = (() => { try { return readIntercompanyMappingState(reader, workspace, input.mappingId!); } finally { reader.close(); } })();
      if (!mapping || mapping.status !== "approved") throw new Error("approved intercompany mapping was not found");
      const actor = actorInput(ctx);
      assertMappingCompaniesAuthorize(workspace, mapping.mapping, actor.createdBy, "group propose-elimination");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...proposeBalanceElimination(db, workspace, { id: String(input.id ?? ""), mappingId: input.mappingId, asOf: String(input.asOf ?? ""), evidenceRefs: input.evidenceRefs }, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation elimination proposal failed"] }); }
  });
  for (const operation of ["approve", "reject", "apply", "reverse"] as const) {
    dispatch.on("group", `${operation}-elimination`, (ctx) => {
      if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: [`--confirm yes required to ${operation} consolidation elimination`] }); return; }
      try {
        const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
        const eliminationId = required(ctx, "--elimination-id");
        const payloadHash = required(ctx, "--payload-hash");
        if (!/^[0-9a-f]{64}$/.test(payloadHash)) throw new Error("--payload-hash must be a lowercase SHA-256 digest");
        const reader = openWorkspaceControlReadOnlyDb(workspace);
        const state = (() => { try { return readBalanceEliminationState(reader, eliminationId); } finally { reader.close(); } })();
        if (!state || state.payloadHash !== payloadHash) throw new Error("exact consolidation elimination state was not found");
        const expected = operation === "approve" || operation === "reject" ? "proposed" : operation === "apply" ? "approved" : "applied";
        if (state.status !== expected) throw new Error(`consolidation elimination must be ${expected} before ${operation}`);
        const actor = actorInput(ctx);
        assertCompanySlugsAuthorize(workspace, [state.payload.left.companySlug, state.payload.right.companySlug], actor.createdBy, `group ${operation}-elimination`);
        const db = openWorkspaceControlDb(workspace);
        try {
          const result = operation === "approve" ? approveBalanceElimination(db, workspace, eliminationId, payloadHash, actor)
            : operation === "reject" ? rejectBalanceElimination(db, eliminationId, payloadHash, actor)
            : operation === "apply" ? applyBalanceElimination(db, workspace, eliminationId, payloadHash, actor)
            : reverseBalanceElimination(db, eliminationId, payloadHash, actor);
          ctx.emitResult({ ok: true, ...result });
        } finally { db.close(); }
      } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : `consolidation elimination ${operation} failed`] }); }
    });
  }
  dispatch.on("group", "eliminations", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try { ctx.emitResult({ ok: true, scope: "consolidation-eliminations", asOf: required(ctx, "--as-of"), eliminations: readAppliedBalanceEliminations(db, required(ctx, "--as-of")) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation eliminations unavailable"] }); }
  });
  dispatch.on("group", "validate-profile", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try {
        const groups = readCurrentGroupManifest(db, workspace);
        if (!groups) throw new Error("group structure must be configured before consolidation profiles");
        ctx.emitResult({ ok: true, profile: parseConsolidationProfile(readManifest(required(ctx, "--profile")), groups.manifest) });
      } finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation profile validation failed"] }); }
  });
  dispatch.on("group", "propose-profile", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to propose consolidation profile"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const input = readManifest(required(ctx, "--profile"));
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      let profile: ConsolidationProfile;
      let manifest: GroupManifest;
      try {
        const groups = readCurrentGroupManifest(reader, workspace);
        if (!groups) throw new Error("group structure must be configured before consolidation profiles");
        manifest = groups.manifest;
        profile = parseConsolidationProfile(input, groups.manifest);
      } finally { reader.close(); }
      const actor = actorInput(ctx);
      assertCompanySlugsAuthorize(workspace, profileCompanySlugs(profile, manifest), actor.createdBy, "group propose-profile");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...proposeConsolidationProfile(db, workspace, input, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation profile proposal failed"] }); }
  });
  dispatch.on("group", "approve-profile", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to approve consolidation profile"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const profileId = required(ctx, "--profile-id");
      const profileHash = required(ctx, "--profile-hash");
      if (!/^[0-9a-f]{64}$/.test(profileHash)) throw new Error("--profile-hash must be a lowercase SHA-256 digest");
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      const { state, manifest } = (() => { try { return { state: readConsolidationProfileState(reader, workspace, profileId), manifest: readCurrentGroupManifest(reader, workspace)?.manifest ?? null }; } finally { reader.close(); } })();
      if (!state || state.status !== "proposed" || state.profileHash !== profileHash) throw new Error("exact pending consolidation profile was not found");
      if (!manifest) throw new Error("group structure is unavailable");
      const actor = actorInput(ctx);
      assertCompanySlugsAuthorize(workspace, profileCompanySlugs(state.profile, manifest), actor.createdBy, "group approve-profile");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...approveConsolidationProfile(db, workspace, profileId, profileHash, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation profile approval failed"] }); }
  });
  dispatch.on("group", "revoke-profile", (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to revoke consolidation profile"] }); return; }
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const profileId = required(ctx, "--profile-id");
      const reader = openWorkspaceControlReadOnlyDb(workspace);
      const { state, manifest } = (() => { try { return { state: readConsolidationProfileState(reader, workspace, profileId), manifest: readCurrentGroupManifest(reader, workspace)?.manifest ?? null }; } finally { reader.close(); } })();
      if (!state || state.status !== "approved") throw new Error("approved consolidation profile was not found");
      if (!manifest) throw new Error("group structure is unavailable");
      const actor = actorInput(ctx);
      assertCompanySlugsAuthorize(workspace, profileCompanySlugs(state.profile, manifest), actor.createdBy, "group revoke-profile");
      const db = openWorkspaceControlDb(workspace);
      try { ctx.emitResult({ ok: true, ...revokeConsolidationProfile(db, workspace, profileId, actor) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidation profile revocation failed"] }); }
  });
  dispatch.on("group", "consolidated-report", (ctx) => {
    try {
      const workspace = resolveWorkspaceRoot(required(ctx, "--workspace"));
      const visible = new Set(listWorkspaceCompanies(workspace).filter((company) => !company.archived).map((company) => company.slug));
      const db = openWorkspaceControlReadOnlyDb(workspace);
      try { ctx.emitResult({ ok: true, ...buildConsolidatedReport(db, workspace, visible, required(ctx, "--profile-id"), required(ctx, "--from"), required(ctx, "--as-of")) }); }
      finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : "consolidated report is unavailable"] }); }
  });
}
