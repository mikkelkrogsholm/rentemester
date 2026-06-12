/**
 * MCP-tool for email-levering af fakturaer og paamindelser (#180).
 *
 * Write-irreversible:
 *   - invoice_send_email
 *
 * Sender en udstedt faktura eller en betalingspaamindelse til kundens email
 * via SMTP med PDF'en vedhaeftet, og registrerer afsendelsen append-only.
 * Trust-boundary: SMTP-CREDENTIALS indgaar aldrig i kerne-bogføringstilstanden
 * — de laeses fra config/smtp.json i virksomhedsmappen. Idempotent: en
 * identisk afsendelse genudsendes ikke. Den indbyggede transport koerer i
 * dryRun, saa et MCP-kald aldrig rammer en rigtig server uden eksplicit
 * konfiguration.
 */

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { companyPaths } from "../../core/paths";
import { createSmtpTransport, sendInvoiceEmail, type SmtpConfig } from "../../core/email";
import { envelopeShape, errorEnvelope, wrapCoreResult } from "../envelope";
import {
  withCompanyDbConfirmed,
  resolveIssuedInvoiceDocumentId,
  invoiceNotFoundEnvelope,
  confirmField,
} from "../tool-runtime";

function loadSmtpConfig(
  companyRoot: string,
): { ok: true; config: SmtpConfig } | { ok: false; error: string } {
  const path = join(companyPaths(companyRoot).config, "smtp.json");
  if (!existsSync(path)) {
    return {
      ok: false,
      error: "missing SMTP config: create config/smtp.json (host, port, fromAddress) in the company directory",
    };
  }
  try {
    const parsed = JSON.parse(readFileSync(path, "utf8")) as Partial<SmtpConfig>;
    return {
      ok: true,
      config: {
        host: parsed.host ?? "",
        port: typeof parsed.port === "number" ? parsed.port : Number(parsed.port ?? 0),
        fromAddress: parsed.fromAddress ?? "",
        fromName: parsed.fromName,
        username: parsed.username,
        password: parsed.password,
        dryRun: parsed.dryRun,
      },
    };
  } catch (error) {
    return { ok: false, error: `could not read config/smtp.json: ${(error as Error).message}` };
  }
}

export function registerEmailTools(server: McpServer): void {
  server.registerTool(
    "invoice_send_email",
    {
      title: "Send invoice or reminder by email",
      description:
        "Sender en udstedt faktura eller en betalingspaamindelse til kundens email via SMTP " +
        "med PDF'en vedhaeftet og registrerer afsendelsen append-only. " +
        "Idempotent: en identisk afsendelse genudsendes ikke. " +
        "SMTP-config laeses fra filen config/smtp.json i virksomhedsmappen; credentials gemmes " +
        "aldrig i bogføringstilstanden. config/smtp.json er et JSON-objekt med PAAKRAEVEDE felter " +
        "host (string), port (number) og fromAddress (string), samt VALGFRIE felter fromName, " +
        "username, password og dryRun (boolean). Mangler filen, afvises kaldet med envelopen " +
        "{ ok:false, errors:[\"missing SMTP config: create config/smtp.json ...\"] } — der kastes " +
        "ingen exception. Den indbyggede SMTP-transport koerer KUN i dry-run: med dryRun:true " +
        "registreres afsendelsen uden et netvaerkskald (ok:true); uden dryRun:true fejler et " +
        "rigtigt send med envelopen { ok:false, errors:[\"the built-in SMTP transport runs in " +
        "dryRun only ...\"] }. Aegte levering kraever en injiceret EmailTransport. write-irreversible.",
      inputSchema: {
        company: z
          .string()
          .min(1)
          .describe("Absolute path to the company directory, or a workspace slug."),
        documentId: z
          .number()
          .int()
          .positive()
          .optional()
          .describe("Document ID of the issued invoice to send. Provide this OR invoiceNumber."),
        invoiceNumber: z
          .string()
          .optional()
          .describe("Invoice number of the issued invoice to send. Provide this OR documentId."),
        kind: z
          .enum(["invoice", "reminder"])
          .optional()
          .describe("What to send (default 'invoice'): 'invoice' = the invoice itself; 'reminder' = a payment reminder."),
        to: z
          .string()
          .optional()
          .describe("Optional recipient email address. When omitted, the customer's stored email is used."),
        confirm: confirmField,
      },
      outputSchema: envelopeShape,
      annotations: {
        readOnlyHint: false,
        destructiveHint: false,
        idempotentHint: true,
        openWorldHint: true,
      },
    },
    withCompanyDbConfirmed<{
      company: string;
      documentId?: number;
      invoiceNumber?: string;
      kind?: "invoice" | "reminder";
      to?: string;
      confirm?: boolean;
    }>(server, "invoice_send_email", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) return invoiceNotFoundEnvelope(args);
      const smtp = loadSmtpConfig(args.company);
      if (!smtp.ok) return errorEnvelope(smtp.error);
      const result = sendInvoiceEmail(db, args.company, {
        invoiceDocumentId: id,
        kind: args.kind ?? "invoice",
        to: args.to,
        smtp: smtp.config,
        transport: createSmtpTransport(smtp.config),
      });
      return wrapCoreResult(result);
    }),
  );
}
