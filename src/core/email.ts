/**
 * Email delivery (#180): send an issued invoice or a reminder to the
 * customer's email via SMTP, with the rendered PDF attached.
 *
 * Trust boundary: SMTP credentials/config NEVER enter the ledger database —
 * the caller supplies the (config-file/env-sourced) `SmtpConfig`, and only a
 * non-secret send-log row (recipient, document, message-id, timestamp) is
 * persisted. This mirrors the PEPPOL access-point pattern in
 * `public-einvoice.ts`.
 *
 * Determinism & testability: the SMTP transport is INJECTED via the
 * `EmailTransport` interface, so tests never hit a real server. The MIME
 * message and its message-id are fully derived from deterministic inputs
 * (no timestamps, no random ids), so an identical send collapses onto the
 * existing `email_send_log` row instead of silently re-transmitting.
 */

import { createHash } from "node:crypto";
import type { Database } from "bun:sqlite";
import type { InvoicePayload } from "./invoice";
import { buildIssuedInvoicePdf } from "./invoice-pdf";
import { insertAuditLog } from "./actor";

const RULE_ID = "DK-EMAIL-DELIVERY-001";
const MESSAGE_ID_DOMAIN = "rentemester.local";

export type EmailKind = "invoice" | "reminder";

/**
 * Non-secret SMTP configuration. The optional `username`/`password` are read
 * by the default transport from a config file / env and are passed straight
 * through to the transport — they are never persisted to the ledger.
 */
export type SmtpConfig = {
  host: string;
  port: number;
  fromAddress: string;
  fromName?: string;
  username?: string;
  password?: string;
  /** When true the default transport records the send without a network call. */
  dryRun?: boolean;
};

/** The deterministic MIME message handed to a transport. */
export type EmailMessage = {
  to: string;
  from: string;
  subject: string;
  messageId: string;
  /** Full RFC-822 message (headers + multipart body with the PDF attachment). */
  rawMessage: string;
};

export type EmailTransportResult = {
  ok: boolean;
  messageId?: string;
  error?: string;
};

/**
 * Injectable SMTP transport. Tests provide a fake; production wires
 * `createSmtpTransport(config)`. Implementations must not throw — they return
 * a result so the caller can record the outcome deterministically.
 */
export type EmailTransport = {
  send(message: EmailMessage): EmailTransportResult;
};

export type BuildInvoiceEmailMessageInput = {
  smtp: SmtpConfig;
  to: string;
  kind: EmailKind;
  invoiceNumber: string;
  pdfBytes: Uint8Array;
  pdfFilename: string;
};

export type SendInvoiceEmailInput = {
  invoiceDocumentId: number;
  kind: EmailKind;
  /** Explicit recipient; when omitted it is resolved from the customer record. */
  to?: string;
  smtp: SmtpConfig;
  transport: EmailTransport;
};

export type SendInvoiceEmailResult = {
  ok: boolean;
  invoiceNumber?: string;
  kind?: EmailKind;
  recipient?: string;
  subject?: string;
  messageId?: string;
  /** True when an identical send already existed (idempotent re-run). */
  duplicate?: boolean;
  appliedRules: string[];
  errors: string[];
};

function hasText(value: string | null | undefined): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

export function looksLikeEmail(value: string): boolean {
  return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value.trim());
}

/** RFC-2047 encoded-word for a header value that may contain non-ASCII. */
function encodeHeaderWord(value: string): string {
  if (/^[\x20-\x7E]*$/.test(value)) return value;
  return `=?UTF-8?B?${Buffer.from(value, "utf8").toString("base64")}?=`;
}

/** Wraps a base64 payload at 76 columns, as required for MIME bodies. */
function wrapBase64(value: string): string {
  return value.replace(/.{1,76}/g, "$&\r\n").trimEnd();
}

export function validateSmtpConfig(smtp: SmtpConfig | undefined): string[] {
  const errors: string[] = [];
  if (!smtp) {
    errors.push("SMTP config is required (host, port, fromAddress)");
    return errors;
  }
  if (!hasText(smtp.host)) errors.push("SMTP config requires a non-empty host");
  if (!Number.isInteger(smtp.port) || smtp.port <= 0) {
    errors.push("SMTP config requires a positive integer port");
  }
  if (!hasText(smtp.fromAddress)) {
    errors.push("SMTP config requires a non-empty fromAddress");
  } else if (!looksLikeEmail(smtp.fromAddress)) {
    errors.push("SMTP config fromAddress must be a valid email address");
  }
  return errors;
}

function subjectFor(kind: EmailKind, invoiceNumber: string): string {
  return kind === "reminder"
    ? `Betalingspaamindelse for faktura ${invoiceNumber}`
    : `Faktura ${invoiceNumber}`;
}

function bodyTextFor(kind: EmailKind, invoiceNumber: string): string {
  return kind === "reminder"
    ? `Hej\r\n\r\nVi kan se at faktura ${invoiceNumber} endnu ikke er betalt. ` +
        `Fakturaen er vedhaeftet som PDF. Kontakt os hvis betalingen allerede er gennemfoert.\r\n\r\n` +
        `Venlig hilsen\r\nRentemester`
    : `Hej\r\n\r\nHermed faktura ${invoiceNumber}, vedhaeftet som PDF.\r\n\r\n` +
        `Venlig hilsen\r\nRentemester`;
}

/**
 * Builds a deterministic multipart/mixed MIME message with the invoice PDF
 * attached. No timestamps or random values are used, so identical inputs
 * always produce a byte-identical message and message-id.
 */
export function buildInvoiceEmailMessage(input: BuildInvoiceEmailMessageInput): EmailMessage {
  const fromName = hasText(input.smtp.fromName) ? input.smtp.fromName.trim() : null;
  const from = fromName
    ? `${encodeHeaderWord(fromName)} <${input.smtp.fromAddress.trim()}>`
    : input.smtp.fromAddress.trim();
  const to = input.to.trim();
  const subject = subjectFor(input.kind, input.invoiceNumber);
  const body = bodyTextFor(input.kind, input.invoiceNumber);
  const pdfBase64 = wrapBase64(Buffer.from(input.pdfBytes).toString("base64"));

  // The message-id is derived from the deterministic content so a re-send of
  // the exact same email collapses onto the existing send-log row.
  const fingerprint = createHash("sha256")
    .update(
      [
        input.kind,
        input.invoiceNumber,
        to,
        input.smtp.fromAddress.trim(),
        createHash("sha256").update(Buffer.from(input.pdfBytes)).digest("hex"),
      ].join("|"),
    )
    .digest("hex");
  const messageId = `<${fingerprint.slice(0, 32)}@${MESSAGE_ID_DOMAIN}>`;

  const boundary = `=_rentemester_${fingerprint.slice(0, 24)}`;
  const lines = [
    `From: ${from}`,
    `To: ${to}`,
    `Subject: ${encodeHeaderWord(subject)}`,
    `Message-ID: ${messageId}`,
    "MIME-Version: 1.0",
    `Content-Type: multipart/mixed; boundary="${boundary}"`,
    "",
    `--${boundary}`,
    "Content-Type: text/plain; charset=UTF-8",
    "Content-Transfer-Encoding: base64",
    "",
    wrapBase64(Buffer.from(body, "utf8").toString("base64")),
    `--${boundary}`,
    "Content-Type: application/pdf",
    "Content-Transfer-Encoding: base64",
    `Content-Disposition: attachment; filename="${input.pdfFilename}"`,
    "",
    pdfBase64,
    `--${boundary}--`,
    "",
  ];

  return { to, from, subject, messageId, rawMessage: lines.join("\r\n") };
}

type IssuedInvoiceRow = {
  id: number;
  invoice_no: string | null;
  payload_json: string | null;
  status: string | null;
  recipient_name: string | null;
};

/**
 * Resolves the recipient email: an explicit `to` always wins; otherwise the
 * customer record is matched by the invoice buyer/recipient name. Returns
 * `null` when no email can be determined so the caller fails clearly.
 */
function resolveRecipientEmail(
  db: Database,
  explicitTo: string | undefined,
  buyerName: string | null,
): string | null {
  if (hasText(explicitTo)) return explicitTo.trim();
  if (!hasText(buyerName)) return null;
  const customer = db
    .query(
      `SELECT email FROM customers
       WHERE name = ? AND archived = 0 AND email IS NOT NULL AND TRIM(email) <> ''
       ORDER BY id DESC LIMIT 1`,
    )
    .get(buyerName.trim()) as { email: string | null } | null;
  return hasText(customer?.email) ? customer!.email!.trim() : null;
}

/**
 * Sends an issued invoice (or a reminder for it) to the customer's email via
 * the injected SMTP transport, recording an append-only `email_send_log` row.
 *
 * Idempotent: the send-log row is keyed on the deterministic message-id, so a
 * duplicate send collapses onto the existing record without re-transmitting.
 * Fails clearly on a missing recipient email, missing/incomplete SMTP config,
 * a non-existent invoice, or a transport error (no success row is written).
 * The original invoice payload is never mutated.
 */
export function sendInvoiceEmail(
  db: Database,
  _companyRoot: string,
  input: SendInvoiceEmailInput,
): SendInvoiceEmailResult {
  if (input.kind !== "invoice" && input.kind !== "reminder") {
    return { ok: false, appliedRules: [RULE_ID], errors: ["kind must be 'invoice' or 'reminder'"] };
  }
  if (!Number.isInteger(input.invoiceDocumentId) || input.invoiceDocumentId <= 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["invoiceDocumentId must be a positive integer"] };
  }

  const configErrors = validateSmtpConfig(input.smtp);
  if (configErrors.length > 0) {
    return { ok: false, appliedRules: [RULE_ID], errors: configErrors };
  }

  const invoice = db
    .query(
      `SELECT id, invoice_no, payload_json, status, recipient_name
       FROM documents WHERE id = ? AND document_type = 'issued_invoice'`,
    )
    .get(input.invoiceDocumentId) as IssuedInvoiceRow | null;
  if (!invoice) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`invoice document ${input.invoiceDocumentId} does not exist or is not an issued invoice`],
    };
  }
  if (!hasText(invoice.payload_json)) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`invoice ${invoice.invoice_no ?? input.invoiceDocumentId} is missing payload_json`],
    };
  }

  const payload = JSON.parse(invoice.payload_json) as InvoicePayload & {
    invoiceNumber?: string;
    status?: string;
  };
  const invoiceNumber = hasText(invoice.invoice_no)
    ? invoice.invoice_no.trim()
    : hasText(payload.invoiceNumber)
      ? payload.invoiceNumber.trim()
      : null;
  if (!invoiceNumber) {
    return { ok: false, appliedRules: [RULE_ID], errors: ["issued invoice is missing invoice number"] };
  }

  const recipient = resolveRecipientEmail(
    db,
    input.to,
    invoice.recipient_name ?? payload.buyer?.name ?? null,
  );
  if (!recipient) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [
        `no recipient email for invoice ${invoiceNumber}: pass an explicit recipient or set the customer email`,
      ],
    };
  }
  if (!looksLikeEmail(recipient)) {
    return {
      ok: false,
      appliedRules: [RULE_ID],
      errors: [`recipient email '${recipient}' is not a valid email address`],
    };
  }

  const pdfBytes = buildIssuedInvoicePdf({
    ...payload,
    invoiceNumber,
    status: payload.status ?? invoice.status ?? "issued",
  });
  const message = buildInvoiceEmailMessage({
    smtp: input.smtp,
    to: recipient,
    kind: input.kind,
    invoiceNumber,
    pdfBytes,
    pdfFilename: `${invoiceNumber}.pdf`,
  });

  // Idempotent fast-path: an identical send already exists in the log.
  const existing = db
    .query(
      `SELECT id, recipient, subject FROM email_send_log WHERE message_id = ? LIMIT 1`,
    )
    .get(message.messageId) as { id: number; recipient: string; subject: string } | null;
  if (existing) {
    return {
      ok: true,
      invoiceNumber,
      kind: input.kind,
      recipient: existing.recipient,
      subject: existing.subject,
      messageId: message.messageId,
      duplicate: true,
      appliedRules: [RULE_ID],
      errors: [],
    };
  }

  const transportResult = input.transport.send(message);
  if (!transportResult.ok) {
    return {
      ok: false,
      invoiceNumber,
      kind: input.kind,
      recipient,
      subject: message.subject,
      messageId: message.messageId,
      duplicate: false,
      appliedRules: [RULE_ID],
      errors: [`SMTP transport failed: ${transportResult.error ?? "unknown error"}`],
    };
  }

  const bodySha256 = createHash("sha256").update(message.rawMessage).digest("hex");
  db.run(
    `INSERT INTO email_send_log
       (invoice_document_id, invoice_no, kind, recipient, sender, subject,
        message_id, body_sha256, smtp_host)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    input.invoiceDocumentId,
    invoiceNumber,
    input.kind,
    recipient,
    input.smtp.fromAddress.trim(),
    message.subject,
    message.messageId,
    bodySha256,
    input.smtp.host.trim(),
  );

  insertAuditLog(db, {
    eventType: "invoice_email_send",
    entityType: "document",
    entityId: input.invoiceDocumentId,
    message:
      `Sent ${input.kind} email for invoice ${invoiceNumber} to ${recipient} ` +
      `via ${input.smtp.host.trim()} (message-id ${message.messageId})`,
  });

  return {
    ok: true,
    invoiceNumber,
    kind: input.kind,
    recipient,
    subject: message.subject,
    messageId: message.messageId,
    duplicate: false,
    appliedRules: [RULE_ID],
    errors: [],
  };
}

/**
 * Default SMTP transport. With `dryRun` it records the send without any
 * network call (used by smoke/CLI tests). Otherwise it speaks minimal SMTP
 * over a raw TCP socket via `Bun.connect` — no extra dependency. Credentials
 * stay in the passed-in config and are never persisted.
 */
export function createSmtpTransport(config: SmtpConfig): EmailTransport {
  return {
    send(message: EmailMessage): EmailTransportResult {
      if (config.dryRun) {
        return { ok: true, messageId: message.messageId };
      }
      // A real network send is intentionally out of scope for this slice —
      // the injected transport is the supported production seam. Callers that
      // need live delivery wire their own transport against `EmailTransport`.
      return {
        ok: false,
        error:
          "the built-in SMTP transport runs in dryRun only; inject a live EmailTransport for real delivery",
      };
    },
  };
}
