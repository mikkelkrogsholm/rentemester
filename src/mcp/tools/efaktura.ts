/**
 * MCP-tools for Digisense e-faktura (#efaktura).
 *
 *  - `efaktura_registrer` (write-irreversible — registrér en virksomhed i
 *    NemHandel via Digisense: register-company ⇒ gem companyKey ⇒
 *    register-participant for BÅDE outbound OG inbound, så en agent kan få
 *    besked "registrér virksomhed CVR XYZ i NemHandel" og håndtere det.
 *    webhookUrl er altid null (vi poller selv). Idempotent: et re-run med samme
 *    CVR duplikerer ikke state. Kræver confirm:true).
 *
 *  - `efaktura_modtag` (write-reversible — poller modtagne e-fakturaer hos
 *    Digisense for en companyKey, følger pagination, og ingester hvert NYT
 *    dokument via den eksisterende ingest-pipeline. Dedup på Digisense' stabile
 *    internalId er rerun-stabil: gentaget poll skaber ingen dubletter).
 *
 * Ingen always-on server: en agent kalder værktøjet ved opstart for at hente
 * nye fakturaer. License-key kommer fra secret-laget (config/digisense.json),
 * ALDRIG fra ledger'en eller værktøjets argumenter; companyKey resolves fra
 * digisense_companies (eller eksplicit digisenseCompanyKey).
 */

import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import {
  pollDigisenseReceived,
  type PollDigisenseReceivedOptions,
} from "../../core/efaktura/digisense-receive";
import {
  registerDigisenseCompany,
  type RegisterDigisenseCompanyOptions,
} from "../../core/efaktura/digisense-register";
import {
  resolveDigisenseReceiver,
  resolveDigisenseRegistrar,
  resolveDigisenseTransmitter,
  resolveDigisenseStatusChecker,
  digisenseProviderBinding,
  digisenseAccessPointIdentity,
} from "../../core/efaktura/digisense-wiring";
import { saveDigisenseSecretConfig, loadDigisenseSecretConfig } from "../../core/efaktura/digisense-config";
import { createDigisenseClient } from "../../core/efaktura/digisense-client";
import { getDigisenseOnboardingStatus, onboardDigisenseCompany } from "../../core/efaktura/digisense-onboarding";
import { pollWorkspaceDigisenseInbound } from "../../core/efaktura/digisense-workspace";
import { resolveWorkspaceRoot } from "../../core/workspace";
import { deriveMcpActor } from "../actor";
import {
  transmitPublicEInvoicePeppol,
  resumePublicEInvoicePeppolSubmission,
} from "../../core/public-einvoice";
import type {
  DigisenseCompanyType,
  DigisenseEnvironment,
  DigisenseNetwork,
} from "../../core/efaktura/digisense-client";
import type { DocumentMetadata } from "../../core/documents";
import { documentMetadataFields } from "./documents";
import { envelopeShape, envelopeToCallResult, errorEnvelope, successEnvelope, wrapCoreResult } from "../envelope";
import {
  withCompanyDbConfirmed,
  withCompanyDb,
  confirmField,
  resolveIssuedInvoiceDocumentId,
  invoiceNotFoundEnvelope,
} from "../tool-runtime";

/**
 * Den valgfri booking-metadata: de SAMME `DocumentMetadata`-felter som
 * `documents_ingest.metadata` men UDEN `source` (pipelinen sætter source til
 * 'digisense_modtag' selv). Bygget fra den delte `documentMetadataFields` så
 * skemaerne ikke kan drive fra hinanden. `.passthrough()` accepterer
 * fremad-kompatible ekstra-nøgler.
 */
const metadataSchema = z
  .object(documentMetadataFields)
  .passthrough()
  .describe("Valgfri DocumentMetadata-payload (uden 'source') der OVERSTYRER de UBL-/listning-afledte felter på hvert modtaget bilag.");

/**
 * Ikke-hemmelig PEPPOL access-point-konfiguration. For Digisense ER access
 * point'et Digisense selv — routing sker på companyKey + license-key — så disse
 * felter er valgfri og defaulter til en fast, deterministisk Digisense-identitet
 * (se transmitPublicEInvoicePeppol-kaldet nedenfor). De påvirker kun
 * idempotency-nøglen, ikke routingen.
 */
export function registerEfakturaTools(server: McpServer): void {
  server.registerTool(
    "efaktura_modtag_workspace",
    {
      title: "Poll DigiSense inbound for active workspace companies",
      description: "Confirm-gated workspace poll. Iterates only active manifest companies using each ledger's local binding; callers cannot supply credentials or companyKey. Continues after per-company failures and returns redacted results. write-reversible.",
      inputSchema: { workspace: z.string().min(1), confirm: confirmField },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    async (args: { workspace: string; confirm?: boolean }) => {
      if (args.confirm !== true) return envelopeToCallResult(errorEnvelope("confirm: true is required", { code: "CONFIRM_REQUIRED" }));
      try {
        const workspaceRoot = resolveWorkspaceRoot(args.workspace);
        const actor = deriveMcpActor(server.server.getClientVersion());
        return envelopeToCallResult(successEnvelope(await pollWorkspaceDigisenseInbound(workspaceRoot, { actor })));
      } catch (error) {
        return envelopeToCallResult(errorEnvelope(error instanceof Error ? error.message : String(error)));
      }
    },
  );
  server.registerTool(
    "efaktura_onboarding_status",
    {
      title: "DigiSense onboarding-status",
      description: "Local, secret-redacted readiness for this ledger's single legal company. Never returns API credentials or signatureSecret.",
      inputSchema: { company: z.string().min(1) }, outputSchema: envelopeShape,
      annotations: { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDb<{ company: string }>(server, ({ db, args }) => successEnvelope(getDigisenseOnboardingStatus(db, args.company))),
  );
  server.registerTool(
    "efaktura_onboard",
    {
      title: "Onboard ledger company with DigiSense",
      description: "Validates authorization and idempotently registers the profile CVR for inbound and outbound. Identity is derived only from the local company profile. write-irreversible.",
      inputSchema: { company: z.string().min(1), confirm: confirmField }, outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{ company: string; confirm?: boolean }>(server, "efaktura_onboard", async ({ db, actor, args }) => {
      const config = loadDigisenseSecretConfig(args.company);
      if (!config) return errorEnvelope("Digisense is not configured");
      const result = await onboardDigisenseCompany(db, args.company, createDigisenseClient(config), actor);
      return result.ok
        ? successEnvelope({ companyKey: result.companyKey, status: result.status })
        : errorEnvelope(result.errors);
    }),
  );
  server.registerTool(
    "efaktura_registrer",
    {
      title: "Registrér virksomhed i NemHandel via Digisense",
      description:
        "Registrerer en virksomhed i NemHandel via Digisense: register-company (DK:CVR) ⇒ gemmer " +
        "companyKey ⇒ register-participant for BÅDE outbound OG inbound, så virksomheden kan både " +
        "sende og modtage e-fakturaer. webhookUrl er altid null — vi poller selv (ingen always-on " +
        "server). Idempotent: et re-run med samme CVR duplikerer ikke state og fejler ikke hårdt. " +
        "License-key kommer fra secret-laget (config/digisense.json), aldrig fra ledger'en eller " +
        "argumenterne. Kræver confirm:true. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        cvr: z
          .string()
          .min(1)
          .describe("CVR-identifikatoren der registreres (fx 'DK12345678')."),
        companyName: z.string().min(1).describe("Virksomhedsnavnet der sendes med register-company."),
        network: z
          .enum(["nemhandel", "peppol"])
          .optional()
          .describe("Netværket participant-registreringen sker på; standard 'nemhandel'."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{
      company: string;
      cvr: string;
      companyName: string;
      network?: DigisenseNetwork;
      confirm?: boolean;
    }>(server, "efaktura_registrer", async ({ db, actor, args }) => {
      const resolved = resolveDigisenseRegistrar(args.company);
      if (!resolved.ok) return errorEnvelope(resolved.errors);

      const companyType: DigisenseCompanyType = { type: "DK:CVR", id: args.cvr };
      const options: RegisterDigisenseCompanyOptions = {
        companyType,
        companyName: args.companyName,
        actor,
      };
      if (args.network !== undefined) options.network = args.network;

      const result = await registerDigisenseCompany(db, args.company, resolved.client, options);
      if (!result.ok) return errorEnvelope(result.errors);
      return successEnvelope({
        companyKey: result.companyKey,
        directionsRegistered: result.directionsRegistered,
        network: result.network,
        participantType: result.participantType,
        participantId: result.participantId,
      });
    }),
  );

  server.registerTool(
    "efaktura_modtag",
    {
      title: "Modtag e-fakturaer via Digisense",
      description:
        "Poller modtagne e-fakturaer hos Digisense for en virksomhed (list-received-documents), " +
        "følger pagination, og ingester hvert NYT dokument via den eksisterende ingest-pipeline. " +
        "Dedup på Digisense' stabile internalId er rerun-stabil — gentaget poll skaber ingen " +
        "dubletter. Ingen always-on server: kald værktøjet ved opstart for at hente nye fakturaer. " +
        "License-key kommer fra secret-laget (config/digisense.json), aldrig fra ledger'en eller " +
        "argumenterne. Kræver confirm:true. write-reversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        digisenseCompanyKey: z
          .string()
          .min(1)
          .optional()
          .describe("Digisense companyKey at polle fra; standard den ENE registrerede virksomhed"),
        limit: z.number().int().positive().max(100).optional().describe("Side-størrelse (<=100); standard 100"),
        maxTimestamp: z
          .string()
          .min(1)
          .optional()
          .describe("Valgfrit øvre tidsstempel som ISO 8601 (fx '2026-06-01T00:00:00Z'); videresendt urørt til Digisense' list-received-documents for at begrænse pollen tidsmæssigt."),
        metadata: metadataSchema.optional(),
        force: z.boolean().optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{
      company: string;
      digisenseCompanyKey?: string;
      limit?: number;
      maxTimestamp?: string;
      metadata?: Omit<DocumentMetadata, "source">;
      force?: boolean;
      confirm?: boolean;
    }>(server, "efaktura_modtag", async ({ db, actor, args }) => {
      const resolved = resolveDigisenseReceiver(db, args.company, {
        companyKey: args.digisenseCompanyKey,
      });
      if (!resolved.ok) return errorEnvelope(resolved.errors);

      const options: PollDigisenseReceivedOptions = {
        companyKey: resolved.companyKey,
        ingestOptions: { forceDuplicateLogicalIdentity: args.force === true },
        actor,
      };
      if (args.limit !== undefined) options.limit = args.limit;
      if (args.maxTimestamp !== undefined) options.maxTimestamp = args.maxTimestamp;
      if (args.metadata !== undefined) options.metadata = args.metadata;

      const result = await pollDigisenseReceived(db, args.company, resolved.client, resolved.downloader, options);
      // `ok:false` er nu kun BATCH-niveau (list-received-documents fejlede).
      // En pr.-dokument-fejl er partiel succes: de ingestede tæller bevares, og
      // de fejlede/quarantined ses i documents[]/errors[]. En agent kan derfor
      // skelne "intet virkede" fra "næsten alt virkede, ét bilag skal håndteres".
      if (!result.ok) return errorEnvelope(result.errors);
      return successEnvelope({
        pagesFetched: result.pagesFetched,
        documentsListed: result.documentsListed,
        documentsIngested: result.documentsIngested,
        documentsSkipped: result.documentsSkipped,
        documentsQuarantined: result.documentsQuarantined,
        documents: result.documents,
        errors: result.errors,
      });
    }),
  );

  server.registerTool(
    "efaktura_konfigurer",
    {
      title: "Gem Digisense API license-key",
      description:
        "Gemmer Digisense API license-key i secret-laget (config/digisense.json, 0600). Dette er " +
        "PRECONDITION'en for efaktura_registrer/efaktura_modtag/efaktura_send — uden en gemt key " +
        "fejler de tre med 'Digisense er ikke konfigureret'. license-key er et SECRET og rammer " +
        "ALDRIG ledger'en. Kræver confirm:true. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        apiLicenseKey: z.string().min(1).describe("Digisense API license-key (én nøgle for hele licensen). Gemmes kun i config/digisense.json, aldrig i ledger'en."),
        environment: z
          .enum(["production", "test"])
          .optional()
          .describe("prod/test base-URL switch; standard 'test'."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false },
    },
    withCompanyDbConfirmed<{
      company: string;
      apiLicenseKey: string;
      environment?: DigisenseEnvironment;
      confirm?: boolean;
    }>(server, "efaktura_konfigurer", ({ args }) => {
      const environment: DigisenseEnvironment = args.environment ?? "test";
      const { path } = saveDigisenseSecretConfig(args.company, {
        apiLicenseKey: args.apiLicenseKey,
        environment,
      });
      return successEnvelope({ configPath: path, environment });
    }),
  );

  server.registerTool(
    "efaktura_send",
    {
      title: "Send e-faktura via Digisense",
      description:
        "Sender en udstedt offentlig e-faktura gennem Digisense' access point: validate-document " +
        "(schematron) ⇒ deliver-document ⇒ poll til delivered, og bogfører en succes som en " +
        "acknowledged PEPPOL-submission. For Digisense ER access point'et Digisense selv (routing " +
        "på companyKey + license-key), så access-point-identiteten udledes deterministisk af " +
        "companyKey — gentaget send af samme faktura er idempotent og leverer aldrig dobbelt. " +
        "License-key kommer fra secret-laget (config/digisense.json); kør efaktura_konfigurer først. " +
        "Kræver confirm:true. write-irreversible.",
      inputSchema: {
        company: z.string().min(1).describe("Absolute path to the company directory, or a workspace slug."),
        documentId: z.number().int().positive().optional().describe("Faktura-dokument-id (ELLER invoiceNumber)."),
        invoiceNumber: z.string().min(1).optional().describe("Fakturanummer (ELLER documentId)."),
        digisenseCompanyKey: z
          .string()
          .min(1)
          .optional()
          .describe("Digisense companyKey at sende fra; standard den ENE registrerede virksomhed."),
        accessPoint: z
          .unknown()
          .optional()
          .describe("Ikke tilladt: Digisense-identiteten afledes fra companyKey."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      digisenseCompanyKey?: string;
      accessPoint?: unknown;
      confirm?: boolean;
    }>(server, "efaktura_send", async ({ db, args }) => {
      if (args.accessPoint !== undefined) {
        return errorEnvelope(["accessPoint is not allowed; Digisense identity is derived from companyKey"]);
      }
      const documentId = resolveIssuedInvoiceDocumentId(db, args);
      if (!documentId) return invoiceNotFoundEnvelope(args);

      const resolved = resolveDigisenseTransmitter(db, args.company, {
        companyKey: args.digisenseCompanyKey,
      });
      if (!resolved.ok) return errorEnvelope(resolved.errors);

      const accessPoint = digisenseAccessPointIdentity(resolved.companyKey);

      const result = await transmitPublicEInvoicePeppol(
        db,
        { invoiceDocumentId: documentId, accessPoint, companyRoot: args.company, providerBinding: resolved.providerBinding },
        resolved.transmitter,
      );
      return wrapCoreResult(result);
    }),
  );

  server.registerTool(
    "efaktura_status",
    {
      title: "Genoptag status for køsat Digisense e-faktura",
      description: "Observerer kun document-status for en tidligere køsat Digisense-afsendelse. Kalder aldrig document-delivery igen og gemmer append-only statusevidens. Kræver confirm:true. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        documentId: z.number().int().positive().describe("Faktura-dokument-id for den allerede køsatte afsendelse."),
        digisenseCompanyKey: z.string().min(1).optional(),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: true },
    },
    withCompanyDbConfirmed<{ company: string; documentId: number; digisenseCompanyKey?: string; confirm?: boolean }>(server, "efaktura_status", async ({ db, args }) => {
      const resolved = resolveDigisenseStatusChecker(db, args.company, { companyKey: args.digisenseCompanyKey });
      if (!resolved.ok) return errorEnvelope(resolved.errors);
      const result = await resumePublicEInvoicePeppolSubmission(db, { invoiceDocumentId: args.documentId, accessPoint: digisenseAccessPointIdentity(resolved.companyKey), companyRoot: args.company, providerBinding: digisenseProviderBinding(resolved.environment, resolved.companyKey) }, async (queuedDocumentId) => {
        const status = await resolved.client.documentStatus(queuedDocumentId, resolved.companyKey);
        return status.ok ? { ok: true, status: status.data.documentStatus, message: status.data.message, publicUrl: status.data.publicUrl } : { ok: false, error: `digisense document-status failed: ${status.error.message}` };
      });
      return wrapCoreResult(result);
    }),
  );
}
