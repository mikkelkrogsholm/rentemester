import { constants, fstatSync, openSync, readFileSync, closeSync } from "node:fs";
import { resolveWorkspaceRoot, findWorkspaceCompany } from "../core/workspace";
import { openWorkspaceControlDb } from "../core/workspace-control";
import { runFirstWorkspaceBootstrap } from "../core/workspace-bootstrap";
import { createPrivateBootstrapService } from "../server/better-auth";
import { createBetterAuthRuntime, assertInjectedBetterAuthSecret } from "../server/better-auth";
import { createHttpJsonV1AuthEmailSender } from "../server/auth-email";
import { resolveServerConfig } from "../server/config";
import { createWorkspaceServicePrincipal, revokeWorkspaceServiceCredential, rotateWorkspaceServiceCredential } from "../core/workspace-service-principals";
import { activateWorkspaceUser, grantCompanyMembership, type CompanyRole } from "../core/workspace-access";
import type { CommandDispatch } from "../cli-dispatch";

/** Never include password-file path or contents in a user-facing error. */
export function readPrivateWorkspaceBootstrapPassword(path: string): string {
  let fd: number | undefined;
  try {
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW);
    const stat = fstatSync(fd);
    if (!stat.isFile() || (stat.mode & 0o777) !== 0o600 || stat.size > 4096) throw new Error("unsafe");
    const raw = readFileSync(fd, "utf8");
    const normalized = raw.endsWith("\r\n") ? raw.slice(0, -2) : raw;
    if (!normalized || normalized.includes("\n") || normalized.includes("\r")) throw new Error("unsafe");
    return normalized;
  } catch {
    throw new Error("password file must be a regular 0600 file up to 4 KiB containing one logical line");
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

/** Provider errors can contain URLs, tokens, or implementation details. */
function safeBootstrapFailure(error: unknown): string {
  const message = error instanceof Error ? error.message : "";
  if (message === "password file must be a regular 0600 file up to 4 KiB containing one logical line") return message;
  if (message === "initial company is not an active registered workspace company") return message;
  if (message === "workspace bootstrap requires hosted deployment configuration") return message;
  return "workspace bootstrap could not be completed; correct the hosted configuration or retry the same identity";
}

function localServiceFailure(error: unknown): string {
  const message = error instanceof Error ? error.message : "";
  if (message === "password file must be a regular 0600 file up to 4 KiB containing one logical line") return message;
  if (message === "initial company is not an active registered workspace company") return message;
  return "local service credential operation could not be completed";
}

function localAuth(db: ReturnType<typeof openWorkspaceControlDb>, authSecretPath: string) {
  const secret = readPrivateWorkspaceBootstrapPassword(authSecretPath);
  assertInjectedBetterAuthSecret(secret);
  return createBetterAuthRuntime(db, { secret, baseURL: "http://localhost", trustedOrigins: ["http://localhost"], deploymentMode: "local", useSecureCookies: false });
}

function activeCompany(workspaceRoot: string, companySlug: string) {
  const company = findWorkspaceCompany(workspaceRoot, companySlug);
  if (!company || company.archived) throw new Error("initial company is not an active registered workspace company");
  return company;
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("workspace-access", "bootstrap-first", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to bootstrap the first workspace identity"] });
      return;
    }
    const workspaceRaw = ctx.trimToNull(ctx.arg("--workspace"));
    const companySlug = ctx.trimToNull(ctx.arg("--company"));
    const name = ctx.trimToNull(ctx.arg("--name"));
    const email = ctx.trimToNull(ctx.arg("--email"));
    const passwordPath = ctx.trimToNull(ctx.arg("--password-file"));
    if (!workspaceRaw || !companySlug || !name || !email || !passwordPath) {
      ctx.fatal("workspace-access bootstrap-first requires --workspace, --company, --name, --email and --password-file");
      return;
    }
    try {
      const workspaceRoot = resolveWorkspaceRoot(workspaceRaw!);
      const company = findWorkspaceCompany(workspaceRoot, companySlug!);
      if (!company || company.archived) throw new Error("initial company is not an active registered workspace company");
      // Hosted config and delivery gateway are checked before DB reservation and password read.
      const config = resolveServerConfig({ workspaceRoot });
      if (config.deploymentProfile !== "hosted" || !config.hostedBetterAuth) throw new Error("workspace bootstrap requires hosted deployment configuration");
      const password = readPrivateWorkspaceBootstrapPassword(passwordPath!);
      const sender = createHttpJsonV1AuthEmailSender({
        ...config.hostedBetterAuth.authEmail,
        idempotencySecret: config.hostedBetterAuth.secret,
      });
      const db = openWorkspaceControlDb(workspaceRoot);
      try {
        const service = createPrivateBootstrapService(db, {
          secret: config.hostedBetterAuth.secret,
          secrets: config.hostedBetterAuth.secrets,
          legacySecret: config.hostedBetterAuth.legacySecret,
          baseURL: config.hostedBetterAuth.baseURL,
          trustedOrigins: config.hostedBetterAuth.trustedOrigins,
          deploymentMode: "hosted",
          useSecureCookies: true,
          rateLimitIpHeader: config.hostedBetterAuth.rateLimitIpHeader,
          emailSender: sender,
        });
        const result = await runFirstWorkspaceBootstrap(db, workspaceRoot, service, {
          name: name!, email: email!, password, companySlug: companySlug!,
          createdBy: process.env.RENTEMESTER_ACTOR!,
          createdByProgram: process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli",
        });
        ctx.emitResult({ ok: true, userId: result.userId, reservationStatus: result.phase, workspaceRole: "workspace_owner", companyRole: "owner", companySlug: result.companySlug, verificationNextStep: "check_verification_email" });
      } finally { db.close(); }
    } catch (error) {
      ctx.emitResult({ ok: false, errors: [safeBootstrapFailure(error)] });
    }
  });

  dispatch.on("workspace-access", "bootstrap-local-service", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to bootstrap a local service principal"] });
      return;
    }
    const workspaceRaw = ctx.trimToNull(ctx.arg("--workspace"));
    const companySlug = ctx.trimToNull(ctx.arg("--company"));
    const displayName = ctx.trimToNull(ctx.arg("--display-name"));
    const authSecretPath = ctx.trimToNull(ctx.arg("--auth-secret-file"));
    const role = ctx.trimToNull(ctx.arg("--company-role")) as CompanyRole | null;
    if (!workspaceRaw || !companySlug || !displayName || !authSecretPath || !role || !["owner", "bookkeeper", "reviewer", "reader"].includes(role)) {
      ctx.fatal("workspace-access bootstrap-local-service requires --workspace, --company, --display-name, --company-role, and --auth-secret-file");
      return;
    }
    try {
      const workspaceRoot = resolveWorkspaceRoot(workspaceRaw);
      activeCompany(workspaceRoot, companySlug);
      const db = openWorkspaceControlDb(workspaceRoot);
      try {
        const auth = localAuth(db, authSecretPath);
        const actor = process.env.RENTEMESTER_ACTOR!;
        const issued = await createWorkspaceServicePrincipal(db, auth, { displayName, actor });
        activateWorkspaceUser(db, { userId: issued.serviceAccountId, workspaceRole: "member", createdBy: actor, createdByProgram: process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli" });
        grantCompanyMembership(db, workspaceRoot, { userId: issued.serviceAccountId, companySlug, role, createdBy: actor, createdByProgram: process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli" });
        ctx.emitResult({ ok: true, serviceAccountId: issued.serviceAccountId, credentialId: issued.credentialId, credential: issued.secret, workspaceRole: "member", companyRole: role, companySlug, credentialHandling: "shown_once_store_in_a_secret_manager_then_rotate_or_revoke_by_id" });
      } finally { db.close(); }
    } catch (error) { ctx.emitResult({ ok: false, errors: [localServiceFailure(error)] }); }
  });

  dispatch.on("workspace-access", "local-service-rotate", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to rotate a local service credential"] }); return; }
    const workspaceRaw=ctx.trimToNull(ctx.arg("--workspace")), companySlug=ctx.trimToNull(ctx.arg("--company")), authSecretPath=ctx.trimToNull(ctx.arg("--auth-secret-file")), serviceAccountId=ctx.trimToNull(ctx.arg("--service-account-id")), credentialId=ctx.trimToNull(ctx.arg("--credential-id"));
    if(!workspaceRaw||!companySlug||!authSecretPath||!serviceAccountId||!credentialId){ctx.fatal("workspace-access local-service-rotate requires --workspace, --company, --service-account-id, --credential-id, and --auth-secret-file");return;}
    try { const workspaceRoot=resolveWorkspaceRoot(workspaceRaw);activeCompany(workspaceRoot,companySlug);const db=openWorkspaceControlDb(workspaceRoot);try{const issued=await rotateWorkspaceServiceCredential(db,localAuth(db,authSecretPath),{serviceAccountId,credentialId,actor:process.env.RENTEMESTER_ACTOR!});ctx.emitResult({ok:true,serviceAccountId:issued.serviceAccountId,credentialId:issued.credentialId,credential:issued.secret,credentialHandling:"shown_once_store_in_a_secret_manager"});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[localServiceFailure(error)]});}
  });

  dispatch.on("workspace-access", "local-service-revoke", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") { ctx.emitResult({ ok: false, errors: ["--confirm yes required to revoke a local service credential"] }); return; }
    const workspaceRaw=ctx.trimToNull(ctx.arg("--workspace")), companySlug=ctx.trimToNull(ctx.arg("--company")), authSecretPath=ctx.trimToNull(ctx.arg("--auth-secret-file")), serviceAccountId=ctx.trimToNull(ctx.arg("--service-account-id")), credentialId=ctx.trimToNull(ctx.arg("--credential-id"));
    if(!workspaceRaw||!companySlug||!authSecretPath||!serviceAccountId||!credentialId){ctx.fatal("workspace-access local-service-revoke requires --workspace, --company, --service-account-id, --credential-id, and --auth-secret-file");return;}
    try { const workspaceRoot=resolveWorkspaceRoot(workspaceRaw);activeCompany(workspaceRoot,companySlug);const db=openWorkspaceControlDb(workspaceRoot);try{await revokeWorkspaceServiceCredential(db,localAuth(db,authSecretPath),{serviceAccountId,credentialId,actor:process.env.RENTEMESTER_ACTOR!});ctx.emitResult({ok:true,serviceAccountId,credentialId,revoked:true});}finally{db.close();}}catch(error){ctx.emitResult({ok:false,errors:[localServiceFailure(error)]});}
  });
}
