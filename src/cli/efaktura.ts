/**
 * CLI for Digisense e-faktura (#efaktura).
 *
 *  - `efaktura registrer` — REGISTRÉR en virksomhed i NemHandel via Digisense:
 *    register-company (DK:CVR) ⇒ gem companyKey ⇒ register-participant for BÅDE
 *    outbound OG inbound (så virksomheden kan både sende og modtage). webhookUrl
 *    er altid null (vi poller selv). Idempotent: et re-run med samme CVR
 *    duplikerer ikke state og fejler ikke hårdt. Skriver audit_log. Kræver
 *    `--confirm yes` (skrivende handling).
 *
 *  - `efaktura modtag` — poller modtagne e-fakturaer hos Digisense for en
 *    companyKey (list-received-documents), følger pagination, og ingester hvert
 *    NYT dokument via den eksisterende ingest-pipeline. Dedup på Digisense'
 *    stabile internalId gør gentaget poll idempotent: et allerede modtaget
 *    dokument skaber ingen dublet. Ingen always-on server — en agent kører
 *    kommandoen ved opstart for at hente nye fakturaer.
 *
 * License-key hentes fra secret-laget (config/digisense.json), ALDRIG fra
 * kald-args; companyKey resolves fra digisense_companies (eller
 * --digisense-company-key).
 */

import { readFileSync } from "node:fs";
import { openCommandDb } from "../cli-dispatch";
import { migrate } from "../core/db";
import {
  pollDigisenseReceived,
  type PollDigisenseReceivedOptions,
} from "../core/efaktura/digisense-receive";
import {
  registerDigisenseCompany,
  type RegisterDigisenseCompanyOptions,
} from "../core/efaktura/digisense-register";
import { registerDigisenseTestGln } from "../core/efaktura/digisense-register-test-gln";
import { registerDigisenseTestSender } from "../core/efaktura/digisense-register-test-sender";
import {
  resolveDigisenseReceiver,
  resolveDigisenseRegistrar,
  resolveDigisenseStatusChecker,
} from "../core/efaktura/digisense-wiring";
import { digisenseAccessPointIdentity, digisenseProviderBinding } from "../core/efaktura/digisense-wiring";
import { resumePublicEInvoicePeppolSubmission } from "../core/public-einvoice";
import { saveDigisenseSecretConfig } from "../core/efaktura/digisense-config";
import { loadDigisenseSecretConfig } from "../core/efaktura/digisense-config";
import { createDigisenseClient, type DigisenseCompanyType, type DigisenseEnvironment } from "../core/efaktura/digisense-client";
import { getDigisenseOnboardingStatus, onboardDigisenseCompany } from "../core/efaktura/digisense-onboarding";
import { pollWorkspaceDigisenseInbound } from "../core/efaktura/digisense-workspace";
import { resolveWorkspaceRoot } from "../core/workspace";
import type { DocumentMetadata } from "../core/documents";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("efaktura", "modtag-workspace", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to poll workspace DigiSense inbound"] });
      process.exit(1);
    }
    const workspace = ctx.trimToNull(ctx.arg("--workspace"));
    if (!workspace) {
      ctx.emitResult({ ok: false, errors: ["Missing required --workspace <dir>"] });
      process.exit(2);
    }
    try {
      const createdBy = ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor();
      if (!createdBy) throw new Error("actor required for workspace DigiSense polling");
      const createdByProgram = ctx.cliActorVia ?? process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli";
      ctx.emitResult(await pollWorkspaceDigisenseInbound(resolveWorkspaceRoot(workspace), {
        actor: { createdBy, createdByProgram },
      }) as unknown as Record<string, unknown>);
    } catch (error) {
      ctx.emitResult({ ok: false, errors: [error instanceof Error ? error.message : String(error)] });
      process.exit(1);
    }
  });
  dispatch.on("efaktura", "onboarding-status", (ctx) => {
    const db = openCommandDb(ctx); migrate(db);
    try { ctx.emitResult({ ok: true, ...getDigisenseOnboardingStatus(db, ctx.companyRoot()) }); }
    finally { db.close(); }
  });

  dispatch.on("efaktura", "onboard", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim().toLowerCase() !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to onboard DigiSense"] }); process.exit(1);
    }
    const root = ctx.companyRoot(); const db = openCommandDb(ctx); migrate(db);
    try {
      const config = loadDigisenseSecretConfig(root);
      if (!config) { ctx.emitResult({ ok: false, errors: ["Digisense is not configured"] }); process.exit(1); }
      const result = await onboardDigisenseCompany(db, root, createDigisenseClient(config), {
        createdBy: ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor() ?? undefined,
        createdByProgram: ctx.cliActorVia ?? process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli",
      });
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok) process.exit(1);
    } finally { db.close(); }
  });
  // `efaktura konfigurer` — gem Digisense API license-key i secret-laget
  // (config/digisense.json, 0600). UDEN denne kommando er hele Digisense-
  // overfladen (registrer/modtag/transmit) uopnåelig: de tre operationer
  // starter alle med loadDigisenseSecretConfig og fejler hvis filen mangler.
  // license-key er et SECRET og rammer ALDRIG ledger'en — kun JSON-filen.
  dispatch.on("efaktura", "konfigurer", (ctx) => {
    const confirmValue = (ctx.arg("--confirm") ?? "").trim().toLowerCase();
    if (confirmValue !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to save Digisense API credentials"] });
      process.exit(1);
    }
    const apiLicenseKey = ctx.trimToNull(ctx.arg("--api-license-key") ?? null);
    if (!apiLicenseKey) {
      ctx.emitResult({
        ok: false,
        errors: ["Missing required --api-license-key <secret>"],
      });
      process.exit(2);
    }
    const envRaw = (ctx.arg("--environment") ?? "test").trim().toLowerCase();
    if (envRaw !== "production" && envRaw !== "test") {
      ctx.emitResult({
        ok: false,
        errors: ["--environment must be 'production' or 'test' (default 'test')"],
      });
      process.exit(1);
    }
    const environment = envRaw as DigisenseEnvironment;
    const root = ctx.companyRoot();
    const { path } = saveDigisenseSecretConfig(root, { apiLicenseKey, environment });
    ctx.emitResult({ ok: true, configPath: path, environment });
  });

  // `efaktura registrer` — registrér virksomhed CVR XYZ i NemHandel.
  dispatch.on("efaktura", "registrer", async (ctx) => {
    const cvr = ctx.trimToNull(ctx.arg("--cvr") ?? null);
    const companyName = ctx.trimToNull(ctx.arg("--company-name") ?? null);
    if (!cvr || !companyName) {
      ctx.emitResult({
        ok: false,
        errors: ["Missing required --cvr <DKxxxxxxxx> and --company-name <text>"],
      });
      process.exit(2);
    }
    // `--confirm` er et VALUED flag (--confirm yes), ikke en bar boolean: den
    // delte cli-args BOOLEAN_FLAGS-mængde er append-only og må ikke ændres.
    // Kun den literale "yes" bekræfter registreringen (samme mønster som asset).
    const confirmValue = (ctx.arg("--confirm") ?? "").trim().toLowerCase();
    if (confirmValue !== "yes") {
      ctx.emitResult({
        ok: false,
        errors: ["--confirm yes required to register the company in NemHandel via Digisense"],
      });
      process.exit(1);
    }

    const root = ctx.companyRoot();
    const resolved = resolveDigisenseRegistrar(root);
    if (!resolved.ok) {
      ctx.emitResult({ ok: false, errors: resolved.errors });
      process.exit(1);
    }

    const companyType: DigisenseCompanyType = { type: "DK:CVR", id: cvr };
    const options: RegisterDigisenseCompanyOptions = {
      companyType,
      companyName,
      actor: {
        createdBy: ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor() ?? undefined,
        createdByProgram: ctx.cliActorVia ?? process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli",
      },
    };
    const network = ctx.trimToNull(ctx.arg("--network") ?? null);
    if (network === "nemhandel" || network === "peppol") options.network = network;

    const db = openCommandDb(ctx);
    migrate(db);
    try {
      const result = await registerDigisenseCompany(db, root, resolved.client, options);
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok) process.exit(1);
    } finally {
      db.close();
    }
  });

  // `efaktura registrer-test-gln` has no GLN input: the only permitted GLN is
  // returned by validate-auth for a test license, constrained to the one local
  // company already registered in this ledger.
  dispatch.on("efaktura", "registrer-test-gln", async (ctx) => {
    const confirmValue = (ctx.arg("--confirm") ?? "").trim();
    if (confirmValue !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to register the Digisense test GLN"] });
      process.exit(1);
    }
    const networkValue = ctx.trimToNull(ctx.arg("--network") ?? null) ?? "nemhandel";
    if (networkValue !== "nemhandel" && networkValue !== "peppol") {
      ctx.emitResult({ ok: false, errors: ["--network must be nemhandel or peppol"] });
      process.exit(1);
    }
    const root = ctx.companyRoot();
    let config: ReturnType<typeof loadDigisenseSecretConfig>;
    try {
      config = loadDigisenseSecretConfig(root);
    } catch {
      ctx.emitResult({ ok: false, errors: ["Digisense test configuration is invalid"] });
      process.exit(1);
    }
    if (!config) {
      ctx.emitResult({ ok: false, errors: ["Digisense test configuration is required"] });
      process.exit(1);
    }
    if (config.environment !== "test" || !config.apiLicenseKey?.trim()) {
      ctx.emitResult({ ok: false, errors: ["Digisense test configuration is required"] });
      process.exit(1);
    }
    const db = openCommandDb(ctx);
    try {
      const client = createDigisenseClient({ apiLicenseKey: config.apiLicenseKey, environment: "test" });
      const result = await registerDigisenseTestGln(db, client, networkValue);
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok) process.exit(1);
    } finally {
      db.close();
    }
  });

  dispatch.on("efaktura", "registrer-test-afsender", async (ctx) => {
    if ((ctx.arg("--confirm") ?? "").trim() !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to register the Digisense test sender"] });
      process.exit(1);
    }
    const root = ctx.companyRoot();
    let config: ReturnType<typeof loadDigisenseSecretConfig>;
    try {
      config = loadDigisenseSecretConfig(root);
    } catch {
      ctx.emitResult({ ok: false, errors: ["Digisense test configuration is invalid"] });
      process.exit(1);
    }
    if (!config || config.environment !== "test" || !config.apiLicenseKey?.trim()) {
      ctx.emitResult({ ok: false, errors: ["Digisense test configuration is required"] });
      process.exit(1);
    }
    const db = openCommandDb(ctx);
    try {
      const client = createDigisenseClient({ apiLicenseKey: config.apiLicenseKey, environment: "test" });
      const result = await registerDigisenseTestSender(db, client);
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok) process.exit(1);
    } finally {
      db.close();
    }
  });

  dispatch.on("efaktura", "modtag", async (ctx) => {
    // En poll ingester bilag (append-only dokumenter + dedup-rækker + audit_log)
    // og rammer netværket. MCP-pendanten efaktura_modtag er confirm-gatet, så
    // CLI'en kræver SYMMETRISK '--confirm yes' (samme mønster som registrer) —
    // ingen divergerende governance-gate mellem CLI og MCP.
    const confirmValue = (ctx.arg("--confirm") ?? "").trim().toLowerCase();
    if (confirmValue !== "yes") {
      ctx.emitResult({
        ok: false,
        errors: ["--confirm yes required to poll and ingest received e-invoices via Digisense"],
      });
      process.exit(1);
    }

    const root = ctx.companyRoot();
    const db = openCommandDb(ctx);
    migrate(db);
    try {
      const resolved = resolveDigisenseReceiver(db, root, {
        companyKey: ctx.trimToNull(ctx.arg("--digisense-company-key") ?? null) ?? undefined,
      });
      if (!resolved.ok) {
        ctx.emitResult({ ok: false, errors: resolved.errors });
        process.exit(1);
      }

      const options: PollDigisenseReceivedOptions = {
        companyKey: resolved.companyKey,
        ingestOptions: { forceDuplicateLogicalIdentity: ctx.hasFlag("--force") },
        actor: {
          createdBy: ctx.cliActor ?? process.env.RENTEMESTER_ACTOR ?? ctx.inferredMutationActor() ?? undefined,
          createdByProgram: ctx.cliActorVia ?? process.env.RENTEMESTER_ACTOR_VIA ?? "rentemester-cli",
        },
      };

      const limit = ctx.parseOptionalNumber("--limit");
      if (!limit.ok) {
        ctx.emitResult({ ok: false, errors: [limit.error] });
        process.exit(1);
      }
      if (limit.value !== undefined) options.limit = limit.value;

      const maxTimestamp = ctx.trimToNull(ctx.arg("--max-timestamp") ?? null);
      if (maxTimestamp) options.maxTimestamp = maxTimestamp;

      // Valgfri booking-metadata (samme konvention som mail-intake's --metadata):
      // overstyrer de UBL-/listning-afledte felter på hvert ingestet bilag.
      const metadataFile = ctx.arg("--metadata");
      if (metadataFile) {
        let raw: unknown;
        try {
          raw = JSON.parse(readFileSync(metadataFile, "utf8"));
        } catch (error) {
          ctx.emitResult({
            ok: false,
            errors: [`could not read --metadata ${metadataFile}: ${error instanceof Error ? error.message : String(error)}`],
          });
          process.exit(1);
        }
        options.metadata = raw as Omit<DocumentMetadata, "source">;
      }

      const result = await pollDigisenseReceived(db, root, resolved.client, resolved.downloader, options);
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok) process.exit(1);
    } finally {
      db.close();
    }
  });

  const registerDeliveryStatus = (command: "status" | "leveringsstatus") => dispatch.on("efaktura", command, async (ctx) => {
    const confirmValue = (ctx.arg("--confirm") ?? "").trim().toLowerCase();
    if (confirmValue !== "yes") {
      ctx.emitResult({ ok: false, errors: ["--confirm yes required to record Digisense delivery status evidence"] });
      process.exit(1);
    }
    const documentId = Number(ctx.arg("--document-id"));
    if (!Number.isSafeInteger(documentId) || documentId <= 0) {
      ctx.emitResult({ ok: false, errors: ["Missing required --document-id <positive integer>"] });
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openCommandDb(ctx);
    migrate(db);
    try {
      const resolved = resolveDigisenseStatusChecker(db, root, { companyKey: ctx.trimToNull(ctx.arg("--digisense-company-key") ?? null) ?? undefined });
      if (!resolved.ok) { ctx.emitResult({ ok: false, errors: resolved.errors }); process.exit(1); }
      const result = await resumePublicEInvoicePeppolSubmission(db, { invoiceDocumentId: documentId, accessPoint: digisenseAccessPointIdentity(resolved.companyKey), providerBinding: digisenseProviderBinding(resolved.environment, resolved.companyKey) }, async (queuedDocumentId) => {
        const status = await resolved.client.documentStatus(queuedDocumentId, resolved.companyKey);
        return status.ok ? { ok: true, status: status.data.documentStatus, message: status.data.message, publicUrl: status.data.publicUrl } : { ok: false, error: `digisense document-status failed: ${status.error.message}` };
      });
      ctx.emitResult(result as unknown as Record<string, unknown>);
      if (!result.ok || result.status === "failed" || result.status === "uncertain") process.exit(1);
    } finally { db.close(); }
  });
  // Legacy alias; `leveringsstatus` avoids ambiguity with onboarding-status.
  registerDeliveryStatus("status");
  registerDeliveryStatus("leveringsstatus");
}
