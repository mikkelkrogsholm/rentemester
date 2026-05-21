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
import { wrapCoreResult, errorEnvelope } from "../envelope";
import { withCompanyDbConfirmed, resolveIssuedInvoiceDocumentId } from "../tool-runtime";

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
        "med PDF'en vedhaeftet og registrerer afsendelsen append-only. SMTP-config laeses fra " +
        "config/smtp.json; credentials gemmes aldrig i bogføringstilstanden. Idempotent: en " +
        "identisk afsendelse genudsendes ikke. write-irreversible.",
      inputSchema: {
        company: z.string().min(1),
        documentId: z.number().int().positive().optional(),
        invoiceNumber: z.string().optional(),
        kind: z.enum(["invoice", "reminder"]).optional(),
        to: z.string().optional(),
        confirm: z.boolean().optional(),
      },
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
      confirm: boolean;
    }>(server, "invoice_send_email", ({ db, args }) => {
      const id = resolveIssuedInvoiceDocumentId(db, args);
      if (!id) {
        return errorEnvelope(
          `Could not resolve invoice: provide documentId or invoiceNumber (got documentId=${args.documentId ?? "-"}, invoiceNumber='${args.invoiceNumber ?? ""}')`,
        );
      }
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
