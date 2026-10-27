/** Concrete per-company bindings for recurring delivery channels. */
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import type { Database } from "bun:sqlite";
import { companyPaths } from "./paths";
import {
  createSmtpTransport,
  sendInvoiceEmail,
  validateSmtpConfig,
  looksLikeEmail,
  type SmtpConfig,
} from "./email";
import {
  resolveDigisenseStatusChecker,
  digisenseProviderBinding,
  resolveDigisenseTransmitter,
  digisenseAccessPointIdentity,
} from "./efaktura/digisense-wiring";
import {
  resumePublicEInvoicePeppolSubmission,
  transmitPublicEInvoicePeppol,
  type SubmitPublicEInvoicePeppolResult,
} from "./public-einvoice";
import type { RecurringDeliveryAdapter, RecurringDeliveryOutcome } from "./recurring-runner";

type LoadedSmtp = { ok: true; config: SmtpConfig } | { ok: false; error: string };

type AdapterDependencies = {
  resolveTransmitter?: typeof resolveDigisenseTransmitter;
  resolveStatusChecker?: typeof resolveDigisenseStatusChecker;
  transmit?: typeof transmitPublicEInvoicePeppol;
  resume?: typeof resumePublicEInvoicePeppolSubmission;
};

function loadSmtpConfig(companyRoot: string): LoadedSmtp {
  const path = join(companyPaths(companyRoot).config, "smtp.json");
  if (!existsSync(path)) return { ok: false, error: "SMTP is not configured for this company" };
  try {
    const parsed = JSON.parse(readFileSync(path, "utf8")) as Partial<SmtpConfig>;
    const config: SmtpConfig = {
      host: parsed.host ?? "",
      port: typeof parsed.port === "number" ? parsed.port : Number(parsed.port ?? 0),
      fromAddress: parsed.fromAddress ?? "",
      fromName: parsed.fromName,
      username: parsed.username,
      password: parsed.password,
      dryRun: parsed.dryRun,
    };
    const errors = validateSmtpConfig(config);
    return errors.length === 0 ? { ok: true, config } : { ok: false, error: errors.join("; ") };
  } catch {
    return { ok: false, error: "SMTP configuration could not be read" };
  }
}

function emailPreflight(db: Database, documentId: number): { ok: boolean; error?: string } {
  const row = db.query(
    `SELECT recipient_name, payload_json FROM documents
      WHERE id = ? AND document_type = 'issued_invoice'`,
  ).get(documentId) as { recipient_name: string | null; payload_json: string | null } | null;
  if (!row) return { ok: false, error: "issued invoice document does not exist" };
  let buyerName = row.recipient_name?.trim() ?? "";
  if (!buyerName && row.payload_json) {
    try { buyerName = (JSON.parse(row.payload_json) as { buyer?: { name?: string } }).buyer?.name?.trim() ?? ""; }
    catch { return { ok: false, error: "issued invoice payload is invalid" }; }
  }
  const customer = buyerName ? db.query(
    `SELECT email FROM customers
      WHERE name = ? AND archived = 0 AND email IS NOT NULL AND TRIM(email) <> ''
      ORDER BY id DESC LIMIT 1`,
  ).get(buyerName) as { email: string | null } | null : null;
  if (!customer?.email || !looksLikeEmail(customer.email)) {
    return { ok: false, error: "invoice recipient has no valid customer email" };
  }
  return { ok: true };
}

function mapPeppolResult(
  result: SubmitPublicEInvoicePeppolResult,
  fallbackProviderId?: string,
): RecurringDeliveryOutcome {
  const providerId = result.transmissionId ?? fallbackProviderId;
  // A failed status/auth/config check may still carry `status: prepared` and
  // the accepted provider id. Treat the errors before the benign pending
  // mapping so the workspace cannot report a successful observation.
  if (!result.ok || result.errors.length > 0) {
    const message = result.errors.join("; ") || "Digisense status observation failed";
    return fallbackProviderId
      ? { status: "accepted_pending", providerId, observationFailed: true, message }
      : { status: "uncertain", providerId, message };
  }
  if (result.status === "acknowledged") {
    return { status: "acknowledged", providerId, message: "Digisense delivery acknowledged" };
  }
  if (result.status === "failed") {
    return { status: "terminal_failed", providerId, message: "Digisense accepted the document but delivery failed terminally" };
  }
  if (result.status === "prepared" && providerId) {
    return { status: "accepted_pending", providerId, message: "Digisense accepted the document and delivery is pending" };
  }
  if (result.status === "uncertain") {
    return { status: "uncertain", providerId, message: "Digisense delivery outcome is uncertain" };
  }
  return {
    status: "uncertain",
    providerId,
    message: result.errors.join("; ") || "Digisense delivery did not reach an acknowledged state",
  };
}

/**
 * Resolves config/state for exactly one already-open company ledger. Resolution
 * itself performs no network call. The returned email binding deliberately
 * supports dry-run only; production supplies a separate adapter at the runner
 * seam when a reviewed live transport exists.
 */
export function resolveRecurringDeliveryAdapter(
  db: Database,
  companyRoot: string,
  dependencies: AdapterDependencies = {},
): RecurringDeliveryAdapter {
  const resolveTransmitter = dependencies.resolveTransmitter ?? resolveDigisenseTransmitter;
  const resolveStatusChecker = dependencies.resolveStatusChecker ?? resolveDigisenseStatusChecker;
  const transmit = dependencies.transmit ?? transmitPublicEInvoicePeppol;
  const resume = dependencies.resume ?? resumePublicEInvoicePeppolSubmission;
  let smtp: LoadedSmtp | undefined;
  let digisense: ReturnType<typeof resolveDigisenseTransmitter> | undefined;
  return {
    async preflight({ documentId, channel }) {
      if (channel === "email") {
        smtp ??= loadSmtpConfig(companyRoot);
        if (!smtp.ok) return { ok: false, error: smtp.error };
        if (smtp.config.dryRun !== true) {
          return {
            ok: false,
            error: "live email transport is unavailable; inject a reviewed production delivery adapter",
          };
        }
        return emailPreflight(db, documentId);
      }
      digisense ??= resolveTransmitter(db, companyRoot);
      return digisense.ok ? { ok: true } : { ok: false, error: digisense.errors.join("; ") };
    },
    async deliver({ documentId, channel }) {
      if (channel === "email") {
        smtp ??= loadSmtpConfig(companyRoot);
        if (!smtp.ok || smtp.config.dryRun !== true) {
          return { status: "uncertain", message: "live email transport is unavailable" };
        }
        const result = sendInvoiceEmail(db, companyRoot, {
          invoiceDocumentId: documentId,
          kind: "invoice",
          smtp: smtp.config,
          transport: createSmtpTransport(smtp.config),
        });
        return result.ok
          ? { status: "acknowledged", message: result.duplicate ? "email already acknowledged" : "email acknowledged" }
          : { status: "uncertain", message: result.errors.join("; ") };
      }

      digisense ??= resolveTransmitter(db, companyRoot);
      if (!digisense.ok) return { status: "uncertain", message: digisense.errors.join("; ") };
      const result = await transmit(db, {
        invoiceDocumentId: documentId,
        companyRoot,
        providerBinding: digisense.providerBinding,
        accessPoint: digisenseAccessPointIdentity(digisense.companyKey),
      }, digisense.transmitter);
      return mapPeppolResult(result);
    },
    async observePending({ documentId, channel, providerId }) {
      if (channel !== "digisense") {
        return { status: "uncertain", providerId, message: "email delivery has no status-only observation contract" };
      }
      const resolved = resolveStatusChecker(db, companyRoot);
      if (!resolved.ok) return {
        status: "accepted_pending",
        providerId,
        observationFailed: true,
        message: resolved.errors.join("; "),
      };
      const result = await resume(db, {
        invoiceDocumentId: documentId,
        companyRoot,
        providerBinding: digisenseProviderBinding(resolved.environment, resolved.companyKey),
        accessPoint: digisenseAccessPointIdentity(resolved.companyKey),
      }, async (documentId) => {
        const status = await resolved.client.documentStatus(documentId, resolved.companyKey);
        return status.ok
          ? {
              ok: true,
              status: status.data.documentStatus,
              message: status.data.message,
              publicUrl: status.data.publicUrl,
            }
          : { ok: false, error: `digisense document-status failed: ${status.error.message}` };
      });
      return mapPeppolResult(result, providerId);
    },
  };
}
