// Invoice write handlers (#213 slice 4, #412, #428, #429, #434, #440).

import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";
import {
  computeInvoiceAmounts,
  type InvoicePayload,
} from "../../core/invoice";
import { issueInvoice, previewIssuedInvoicePdf } from "../../core/issued-invoices";
import { postIssuedInvoiceToLedger } from "../../core/invoice-booking";
import { settleInvoiceFromBank } from "../../core/invoice-settlement";
import {
  postInvoiceReminderToLedger,
  registerInvoiceReminder,
} from "../../core/invoice-reminders";
import { issueCreditNote } from "../../core/credit-notes";
import { resolveInvoiceMasterData } from "../../core/master-data";
import {
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
} from "../../core/public-einvoice";
import {
  createSmtpTransport,
  looksLikeEmail,
  sendInvoiceEmail,
  validateSmtpConfig,
  type EmailKind,
  type SmtpConfig,
} from "../../core/email";
import { companyPaths } from "../../core/paths";
import { migrate, openDb } from "../../core/db";
import {
  companyRootForSlug,
  findWorkspaceCompany,
} from "../../core/workspace";
import { authMiddleware } from "../auth";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCockpitActor } from "../actor";
import { withCompanyMutation } from "../mutations";
import {
  okResponse,
  optionalBodyBoolean,
  optionalBodyNumber,
  optionalBodyPositiveInt,
  optionalBodyString,
  parseInvoiceLines,
  parseInvoiceParty,
  requireBodyPositiveInt,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/invoices/issue — issues a sales invoice.
 *
 * Body: `{ issueDate: string, lines: [{description, quantity, unitPriceExVat}],
 * vatRatePercent?: number, customerId?: number, buyer?: {name,address,vatOrCvr},
 * seller?: {name,address,vatOrCvr}, invoiceNumber?: string, dueDate?: string,
 * currency?: string }`.
 *
 * Rentemester COMPUTES every line total, the net amount, the VAT amount and
 * the gross amount from the human's minimal input — the exact compute path of
 * the CLI's guided `invoice create` command (`computeInvoiceAmounts` →
 * `issueInvoice`). The human never hand-writes an amount. A stored
 * `customerId` back-fills the buyer from master data, exactly as the CLI does.
 *
 * Issuing produces a kladde (no journal entry yet), so it is NOT marked
 * `requireConfirm` — the multi-line modal IS the human's consent. The route
 * still goes through `withCompanyMutation` for the backup lock, the localhost
 * gate and actor attribution.
 */
export async function handleInvoiceIssue(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const issueDate = requireBodyString(body, "issueDate");
      const lines = parseInvoiceLines(body.lines);
      const vatRatePercent = optionalBodyNumber(body, "vatRatePercent") ?? 25;
      const customerId = optionalBodyPositiveInt(body, "customerId");
      const invoiceNumber = optionalBodyString(body, "invoiceNumber");
      const dueDate = optionalBodyString(body, "dueDate");
      const currency = optionalBodyString(body, "currency");

      // The human types either a stored --customer-id or the buyer details
      // directly — same as `invoice create`. `parseInvoiceParty` keeps a
      // partially-filled party object so master-data / validation can fill or
      // reject it.
      const buyer = parseInvoiceParty(body.buyer, "buyer");
      const seller = parseInvoiceParty(body.seller, "seller");

      // Rentemester computes every derived amount — the human never does
      // invoice arithmetic. A compute rejection (blank line, bad quantity) is
      // a core `{ok:false}` and surfaces as a 400 via `withCompanyMutation`.
      const computed = computeInvoiceAmounts(lines, vatRatePercent);
      if (!computed.ok) {
        return { ok: false, errors: computed.errors };
      }

      const payload: InvoicePayload = {
        invoiceType: "full",
        vatTreatment: "standard",
        issueDate,
        ...(invoiceNumber ? { invoiceNumber } : {}),
        seller,
        buyer,
        lines: computed.lines,
        totals: {
          netAmount: computed.totals.netAmount,
          vatRate: computed.totals.vatRate,
          vatAmount: computed.totals.vatAmount,
          grossAmount: computed.totals.grossAmount,
        },
        currency: currency ?? "DKK",
        ...(dueDate ? { dueDate } : {}),
      };

      // Master-data resolution mirrors the CLI: a given `customerId` back-fills
      // the buyer name/address/VAT from the registered customer.
      const resolved = resolveInvoiceMasterData(ctx.db, payload, { customerId });
      if (!resolved.ok) {
        return { ok: false, errors: resolved.errors ?? ["master-data resolution failed"] };
      }
      const issued = issueInvoice(ctx.db, ctx.companyRoot, resolved.payload);
      return {
        ok: issued.ok,
        errors: issued.errors,
        documentId: issued.documentId,
        invoiceNumber: issued.invoiceNumber,
        computed,
      };
    },
  );

  // Echo the computed amounts so the modal can show the human exactly what
  // Rentemester worked out from their input.
  return okResponse({
    invoice: {
      documentId: result.documentId ?? null,
      invoiceNumber: result.invoiceNumber ?? null,
      netAmount: result.computed?.totals?.netAmount ?? 0,
      vatRate: result.computed?.totals?.vatRate ?? 0,
      vatAmount: result.computed?.totals?.vatAmount ?? 0,
      grossAmount: result.computed?.totals?.grossAmount ?? 0,
      lines: result.computed?.lines ?? [],
    },
  });
}

/**
 * POST /api/companies/:slug/invoices/preview — render an invoice PDF without
 * writing anything to the ledger (#440).
 *
 * Body: identical shape to `handleInvoiceIssue` (issueDate, lines, vatRatePercent,
 * customerId, buyer, seller, ...). Runs the SAME compute + validate + enrich
 * + buyer-master-data resolution as `issueInvoice`, then calls
 * `previewIssuedInvoicePdf` instead — no document row, no audit_log, no
 * sequence draw. Returns the raw PDF bytes with Content-Type application/pdf,
 * exactly like `GET .../invoices/:id/pdf`.
 *
 * Does NOT go through `withCompanyMutation`: this is a pure read+render. The
 * backup-lock gate and the confirm gate are irrelevant — nothing is mutated —
 * and the audit-log actor attribution is similarly irrelevant. The localhost
 * hard-gate IS still enforced (Phase-1 trust); the company-resolution +
 * db-open pattern is the same shape as the mutation pipeline, minus those
 * gates.
 */
export async function handleInvoicePreview(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  // Phase-1 localhost trust + the auth seam — kept identical to the write
  // pipeline so the preview cannot be probed by a non-loopback client.
  authMiddleware(request, config);
  if (!config.authRequired) {
    const hostHeader = (request.headers.get("host") ?? "").trim().toLowerCase();
    const host = hostHeader.startsWith("[")
      ? hostHeader.slice(
          1,
          hostHeader.indexOf("]") === -1 ? undefined : hostHeader.indexOf("]"),
        )
      : (hostHeader.split(":")[0] ?? "");
    const isLoopback =
      host === "127.0.0.1" ||
      host === "localhost" ||
      host === "::1" ||
      host === "0:0:0:0:0:0:0:1";
    if (!isLoopback) {
      throw ApiError.unauthorized(
        "Forhåndsvisning fra Cockpit er kun tilladt fra localhost, " +
          "medmindre godkendelse er slået til.",
      );
    }
  }

  if (!findWorkspaceCompany(config.workspaceRoot, slug)) {
    throw ApiError.notFound(`ingen virksomhed med slug '${slug}' findes i workspacet`);
  }
  const companyRoot = companyRootForSlug(config.workspaceRoot, slug);
  const dbPath = companyPaths(companyRoot).db;
  if (!existsSync(dbPath)) {
    throw ApiError.notFound(`virksomheden '${slug}' har ingen ledger`);
  }

  const raw = await request.text();
  let body: Record<string, unknown>;
  if (raw.trim().length === 0) {
    body = {};
  } else {
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      throw ApiError.badRequest("request body must be valid JSON");
    }
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw ApiError.badRequest("request body must be a JSON object");
    }
    body = parsed as Record<string, unknown>;
  }

  const issueDate = requireBodyString(body, "issueDate");
  const lines = parseInvoiceLines(body.lines);
  const vatRatePercent = optionalBodyNumber(body, "vatRatePercent") ?? 25;
  const customerId = optionalBodyPositiveInt(body, "customerId");
  const invoiceNumber = optionalBodyString(body, "invoiceNumber");
  const dueDate = optionalBodyString(body, "dueDate");
  const currency = optionalBodyString(body, "currency");
  const buyer = parseInvoiceParty(body.buyer, "buyer");
  const seller = parseInvoiceParty(body.seller, "seller");

  const computed = computeInvoiceAmounts(lines, vatRatePercent);
  if (!computed.ok) {
    throw ApiError.badRequest(computed.errors.join("; "));
  }

  const payload: InvoicePayload = {
    invoiceType: "full",
    vatTreatment: "standard",
    issueDate,
    ...(invoiceNumber ? { invoiceNumber } : {}),
    seller,
    buyer,
    lines: computed.lines,
    totals: {
      netAmount: computed.totals.netAmount,
      vatRate: computed.totals.vatRate,
      vatAmount: computed.totals.vatAmount,
      grossAmount: computed.totals.grossAmount,
    },
    currency: currency ?? "DKK",
    ...(dueDate ? { dueDate } : {}),
  };

  const db = openDb(dbPath);
  try {
    migrate(db);
    const resolved = resolveInvoiceMasterData(db, payload, { customerId });
    if (!resolved.ok) {
      const message = (resolved.errors ?? ["master-data resolution failed"]).join("; ");
      if (/does not exist|findes ikke|not found|allerede|already/i.test(message)) {
        throw ApiError.conflict(message);
      }
      throw ApiError.badRequest(message);
    }
    const preview = previewIssuedInvoicePdf(db, resolved.payload);
    if (!preview.ok) {
      throw ApiError.badRequest(preview.errors.join("; "));
    }
    // PDF bytes inline — same Content-Type / cache headers as the real
    // `GET .../invoices/:id/pdf` route so the cockpit can open the response
    // in a new tab via window.open()/URL.createObjectURL.
    return new Response(preview.pdfBytes, {
      status: 200,
      headers: {
        "content-type": "application/pdf",
        "content-disposition": `inline; filename*=UTF-8''${encodeURIComponent(
          `${preview.invoiceNumber}.pdf`,
        )}`,
        "x-content-type-options": "nosniff",
        "cache-control": "private, no-store",
      },
    });
  } finally {
    db.close();
  }
}

/**
 * POST /api/companies/:slug/invoices/post — posts an issued invoice to the
 * ledger.
 *
 * Body: `{ invoiceDocumentId: number, transactionDate?: string,
 * confirm: true }`. Calls the SAME `postIssuedInvoiceToLedger` core function
 * the CLI's `invoice post` command uses.
 *
 * Write-irreversible — it appends a journal entry — so `requireConfirm` is
 * set. A double-post is refused by core (`invoice X already has journal entry
 * Y`), which `withCompanyMutation` maps to a 409 conflict.
 */
export async function handleInvoicePost(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const transactionDate = optionalBodyString(body, "transactionDate");
      const posted = postIssuedInvoiceToLedger(
        ctx.db,
        withCockpitActor(
          { invoiceDocumentId, transactionDate },
          ctx.actor,
        ),
      );
      return {
        ok: posted.ok,
        errors: posted.errors,
        entryId: posted.entryId,
        entryNo: posted.entryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    posting: {
      entryId: result.entryId ?? null,
      entryNo: result.entryNo ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/invoices/settle — settles an issued invoice
 * against a bank payment.
 *
 * Body: `{ invoiceDocumentId: number, bankTransactionId?: number,
 * bankTransactionReference?: string, paymentDate?: string, amount?: number,
 * confirm: true }`. Calls the SAME `settleInvoiceFromBank` core function the
 * CLI's `invoice settle-bank` command uses.
 *
 * Write-irreversible — it links a bank receipt and appends a journal entry —
 * so `requireConfirm` is set. A double-settle is refused by core (`bank
 * transaction N is already linked …`), mapped to a 409 conflict.
 */
export async function handleInvoiceSettle(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const bankTransactionId = optionalBodyPositiveInt(body, "bankTransactionId");
      const bankTransactionReference = optionalBodyString(
        body,
        "bankTransactionReference",
      );
      const paymentDate = optionalBodyString(body, "paymentDate");
      const amount = optionalBodyNumber(body, "amount");
      if (bankTransactionId === undefined && bankTransactionReference === undefined) {
        throw ApiError.badRequest(
          "'bankTransactionId' or 'bankTransactionReference' is required",
        );
      }
      const settled = settleInvoiceFromBank(
        ctx.db,
        withCockpitActor(
          {
            invoiceDocumentId,
            bankTransactionId,
            bankTransactionReference,
            paymentDate,
            amount,
          },
          ctx.actor,
        ),
      );
      return {
        ok: settled.ok,
        errors: settled.errors,
        entryId: settled.entryId,
        paymentId: settled.paymentId,
        principalAmount: settled.principalAmount,
        claimAmount: settled.claimAmount,
        invoiceNumber: settled.invoiceNumber,
        openBalance: settled.openBalance,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    settlement: {
      entryId: result.entryId ?? null,
      paymentId: result.paymentId ?? null,
      principalAmount: result.principalAmount ?? 0,
      claimAmount: result.claimAmount ?? 0,
      invoiceNumber: result.invoiceNumber ?? null,
      openBalance: result.openBalance ?? null,
    },
  });
}

/**
 * POST /api/companies/:slug/invoices/credit-note — issues a credit note for an
 * already-issued sales invoice (#412).
 *
 * Body: `{ invoiceDocumentId: number, issueDate: string, reason: string,
 * grossAmount?: number, creditNoteNumber?: string, confirm: true }`. Calls the
 * SAME `issueCreditNote` core function the CLI's `invoice credit-note` command
 * uses, so the Cockpit and the terminal produce byte-identical credit notes
 * and identical journal reversals.
 *
 * Write-irreversible — it inserts a credit-note document AND appends a
 * reversal journal entry — so `requireConfirm` is set. Re-crediting an already
 * fully credited invoice is refused by core (`invoice X is already fully
 * credited`); the `already` heuristic in `withCompanyMutation` maps that to a
 * 409, and a missing source invoice (`invoice document N does not exist`) to a
 * 409 as well. A blank reason or non-positive amount is rejected before core
 * is reached and surfaces as a 400.
 */
export async function handleInvoiceCreditNote(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const issueDate = requireBodyString(body, "issueDate");
      const reason = requireBodyString(body, "reason");
      const grossAmount = optionalBodyNumber(body, "grossAmount");
      const creditNoteNumber = optionalBodyString(body, "creditNoteNumber");
      const credited = issueCreditNote(
        ctx.db,
        ctx.companyRoot,
        withCockpitActor(
          {
            originalInvoiceDocumentId: invoiceDocumentId,
            issueDate,
            reason,
            ...(grossAmount !== undefined ? { grossAmount } : {}),
            ...(creditNoteNumber ? { creditNoteNumber } : {}),
          },
          ctx.actor,
        ),
      );
      return {
        ok: credited.ok,
        errors: credited.errors,
        documentId: credited.documentId,
        creditNoteNumber: credited.creditNoteNumber,
        originalInvoiceNumber: credited.originalInvoiceNumber,
        journalEntryId: credited.journalEntryId,
        journalEntryNo: credited.journalEntryNo,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    creditNote: {
      documentId: result.documentId ?? null,
      creditNoteNumber: result.creditNoteNumber ?? null,
      originalInvoiceNumber: result.originalInvoiceNumber ?? null,
      journalEntryId: result.journalEntryId ?? null,
      journalEntryNo: result.journalEntryNo ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Send som e-faktura (NemHandel / PEPPOL) — #428.
//
// A SMB owner that invoices a public buyer is required by law to deliver the
// invoice as an e-faktura. Until now the only way to do so from Rentemester
// was the CLI command `invoice submit-public-peppol`, which most owners never
// discover. This handler is the Cockpit's third caller of the SAME
// `submitPublicEInvoicePeppol` core function the CLI/MCP use — so the
// Cockpit and the terminal produce byte-identical PEPPOL envelopes and
// identical `peppol_submissions` rows.
//
// Access-point CONFIG (non-secret: accessPointId + endpointUrl + sender
// endpointId) is read from a file referenced by the `RENTEMESTER_PEPPOL_ACCESS_POINT`
// env var, mirroring how `bun run cli invoice submit-public-peppol` consumes
// its `--access-point <file.json>`. Credentials never enter the request body
// nor the server config object. When the env var is not configured, the
// handler returns a 400 with a clear next-step message — never a 500.
// --------------------------------------------------------------------------

/**
 * Loads the non-secret PEPPOL access-point config from a JSON file at the
 * path in `RENTEMESTER_PEPPOL_ACCESS_POINT`. Returns `null` (not throws) when
 * the env var is missing — that case is mapped to a 400 with a clear
 * next-step so the SMB owner knows what to configure. A malformed file is a
 * 400 with the parse error verbatim.
 */
function loadConfiguredPeppolAccessPoint(): PeppolAccessPointConfig {
  const path = (process.env.RENTEMESTER_PEPPOL_ACCESS_POINT ?? "").trim();
  if (!path) {
    throw ApiError.badRequest(
      "PEPPOL er ikke konfigureret i denne installation. " +
        "Sæt RENTEMESTER_PEPPOL_ACCESS_POINT til stien for en JSON-fil med " +
        "{accessPointId, endpointUrl, senderEndpointId} for at sende e-fakturaer.",
    );
  }
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (error) {
    throw ApiError.badRequest(
      `PEPPOL access-point-config kunne ikke læses fra ${path}: ${(error as Error).message}`,
    );
  }
  let parsed: {
    accessPointId?: string;
    endpointUrl?: string;
    senderEndpointId?: string;
  };
  try {
    parsed = JSON.parse(raw) as typeof parsed;
  } catch (error) {
    throw ApiError.badRequest(
      `PEPPOL access-point-config er ikke gyldig JSON: ${(error as Error).message}`,
    );
  }
  return {
    accessPointId: (parsed.accessPointId ?? "").trim(),
    endpointUrl: (parsed.endpointUrl ?? "").trim(),
    senderEndpointId: (parsed.senderEndpointId ?? "").trim(),
  };
}

/**
 * POST /api/companies/:slug/invoices/send-public — sends an issued invoice
 * as a public e-faktura via NemHandel / PEPPOL.
 *
 * Body: `{ invoiceDocumentId: number, confirm: true }`. Calls the SAME
 * `submitPublicEInvoicePeppol` core function the CLI's
 * `invoice submit-public-peppol` command uses, so the Cockpit and the
 * terminal produce byte-identical PEPPOL submission envelopes. Idempotent:
 * a second submission for the same invoice/access-point pair collapses onto
 * the existing `peppol_submissions` row (the underlying core enforces this
 * via a derived idempotency key) and the handler echoes `duplicate: true`.
 *
 * Write-irreversible (it inserts a `peppol_submissions` row AND appends an
 * `audit_log` entry — both write-once tables) so `requireConfirm` is set.
 * Goes through `withCompanyMutation`, so the backup lock, the localhost gate
 * and actor attribution all apply.
 *
 * The access-point CONFIG (non-secret: accessPointId + endpointUrl + sender
 * endpointId) is loaded from `RENTEMESTER_PEPPOL_ACCESS_POINT`; credentials
 * are NEVER passed in the request body. A missing/invalid config is a 400.
 */
export async function handleInvoiceSendPublic(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const accessPoint = loadConfiguredPeppolAccessPoint();

  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      // Touch the resolved actor so the cockpit's submit is attributable in
      // the audit_log entry the core writes (the core itself records the
      // submission as the authenticated actor that opened the db).
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const submitted = submitPublicEInvoicePeppol(ctx.db, {
        invoiceDocumentId,
        accessPoint,
      });
      return {
        ok: submitted.ok,
        errors: submitted.errors,
        invoiceNumber: submitted.invoiceNumber,
        submissionReference: submitted.submissionReference,
        status: submitted.status,
        duplicate: submitted.duplicate,
        envelopeSha256: submitted.envelopeSha256,
        oioublSha256: submitted.oioublSha256,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    submission: {
      invoiceNumber: result.invoiceNumber ?? null,
      submissionReference: result.submissionReference ?? null,
      status: result.status ?? null,
      duplicate: Boolean(result.duplicate),
      envelopeSha256: result.envelopeSha256 ?? null,
      oioublSha256: result.oioublSha256 ?? null,
    },
  });
}

// --------------------------------------------------------------------------
// Send invoice by e-mail (#429).
//
// Before this route the cockpit could only render "Hent PDF" — to actually
// get the invoice to the customer the owner had to download the PDF, open
// Outlook/Gmail/Apple Mail, look up the customer's e-mail and attach the
// file by hand. The fall-back was the CLI command `invoice send`, which
// most owners never discover. This handler is the cockpit's third caller of
// the SAME `sendInvoiceEmail` core function the CLI's `invoice send` and the
// MCP tool `invoice_send_email` use — so the cockpit and the terminal
// produce a byte-identical MIME message and the same `email_send_log` row.
//
// SMTP CONFIG (non-secret host/port/fromAddress + optional credentials) is
// read from a file at `config/smtp.json` inside the company directory,
// mirroring how the CLI command loads its config. Credentials never enter
// the request body nor the server config object. When the file is missing
// the handler returns a 400 with a clear next-step message — never a 500.
// --------------------------------------------------------------------------

/**
 * Loads `config/smtp.json` from the company directory. The file is non-secret
 * config (host/port/fromAddress) plus optional credentials the default SMTP
 * transport reads but never persists. Returns a `400` with a clear next-step
 * when the file is missing or malformed — the cockpit shows that message
 * verbatim so the owner knows what to configure.
 */
function loadConfiguredSmtp(companyRoot: string): SmtpConfig {
  const path = join(companyPaths(companyRoot).config, "smtp.json");
  if (!existsSync(path)) {
    throw ApiError.badRequest(
      "SMTP er ikke konfigureret for denne virksomhed. Opret config/smtp.json " +
        "med {host, port, fromAddress} (og evt. username/password) inde i " +
        "virksomhedsmappen for at sende fakturaer på mail fra cockpittet.",
    );
  }
  let raw: string;
  try {
    raw = readFileSync(path, "utf8");
  } catch (error) {
    throw ApiError.badRequest(
      `SMTP-config kunne ikke læses fra ${path}: ${(error as Error).message}`,
    );
  }
  let parsed: Partial<SmtpConfig>;
  try {
    parsed = JSON.parse(raw) as Partial<SmtpConfig>;
  } catch (error) {
    throw ApiError.badRequest(
      `SMTP-config er ikke gyldig JSON: ${(error as Error).message}`,
    );
  }
  return {
    host: typeof parsed.host === "string" ? parsed.host : "",
    port: typeof parsed.port === "number" ? parsed.port : Number(parsed.port ?? 0),
    fromAddress: typeof parsed.fromAddress === "string" ? parsed.fromAddress : "",
    fromName: typeof parsed.fromName === "string" ? parsed.fromName : undefined,
    username: typeof parsed.username === "string" ? parsed.username : undefined,
    password: typeof parsed.password === "string" ? parsed.password : undefined,
    dryRun: typeof parsed.dryRun === "boolean" ? parsed.dryRun : undefined,
  };
}

/**
 * POST /api/companies/:slug/invoices/send-email — sends an issued invoice
 * (or a reminder for it) to the customer's e-mail via SMTP, with the PDF
 * attached.
 *
 * Body: `{ invoiceDocumentId: number, to: string, kind?: 'invoice' |
 * 'reminder', confirm: true }`. Calls the SAME `sendInvoiceEmail` core
 * function the CLI's `invoice send` command uses, so the cockpit and the
 * terminal produce a byte-identical MIME message and `email_send_log` row.
 * Idempotent: an identical send collapses onto the existing row instead of
 * re-transmitting (the core derives the message-id from the inputs).
 *
 * Write-irreversible (it appends an `email_send_log` row AND an `audit_log`
 * entry — both write-once tables) so `requireConfirm` is set. Goes through
 * `withCompanyMutation`, so the backup lock, the localhost gate and actor
 * attribution all apply.
 *
 * SMTP config (non-secret host/port/fromAddress + optional credentials) is
 * loaded from `config/smtp.json` inside the company directory; credentials
 * are NEVER passed in the request body. A missing/invalid config is a 400.
 */
export async function handleInvoiceSendEmail(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const to = requireBodyString(body, "to");
      const kindRaw = body.kind;
      if (
        kindRaw !== undefined &&
        kindRaw !== "invoice" &&
        kindRaw !== "reminder"
      ) {
        throw ApiError.badRequest(
          "'kind' must be 'invoice' or 'reminder' when present",
        );
      }
      const kind = (kindRaw ?? "invoice") as EmailKind;
      const smtp = loadConfiguredSmtp(ctx.companyRoot);
      const sent = sendInvoiceEmail(ctx.db, ctx.companyRoot, {
        invoiceDocumentId,
        kind,
        to,
        smtp,
        transport: createSmtpTransport(smtp),
      });
      return {
        ok: sent.ok,
        errors: sent.errors,
        invoiceNumber: sent.invoiceNumber,
        recipient: sent.recipient,
        subject: sent.subject,
        messageId: sent.messageId,
        kind: sent.kind,
        duplicate: sent.duplicate,
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    delivery: {
      invoiceNumber: result.invoiceNumber ?? null,
      recipient: result.recipient ?? null,
      subject: result.subject ?? null,
      messageId: result.messageId ?? null,
      duplicate: Boolean(result.duplicate),
    },
  });
}

// --------------------------------------------------------------------------
// Send rykker (betalingspaamindelse) (#434).
//
// Before this route the cockpit could show a row as "Forfalden · 47 dage"
// and surface the customer's e-mail, but there was no way to actually send
// the rykker — the owner had to download the PDF, open the mail client and
// type the reminder by hand, and the cockpit had no record that the rykker
// went out. This handler combines THREE existing core calls in a single
// mutation so the cockpit's "Send rykker" button is a one-click write:
//
//   1. `registerInvoiceReminder` — registers the reminder + statutory fee
//      (max 100 kr/reminder, max 3 reminders per rentel. § 9b),
//   2. (when `bookFee` is true) `postInvoiceReminderToLedger` — journals
//      the fee against the customer receivable so it shows up on the
//      claim balance,
//   3. `sendInvoiceEmail` with `kind: 'reminder'` — same SMTP transport as
//      "Send på mail" (#429), so the message is byte-identical to a CLI
//      send.
//
// Write-irreversible (insert into `invoice_reminders` + optional journal
// entry + always-on `email_send_log` and `audit_log` rows), so the route
// requires `confirm: true`. The SMTP config (non-secret host/port/from +
// optional credentials) is read from `config/smtp.json` inside the company
// directory — credentials never enter the request body.
// --------------------------------------------------------------------------

/**
 * POST /api/companies/:slug/invoices/send-reminder — registers a payment
 * reminder for an overdue issued invoice and sends it on e-mail.
 *
 * Body: `{ invoiceDocumentId: number, to: string, bookFee: boolean,
 * feeAmount?: number, confirm: true }`. `bookFee=true` also journals the
 * statutory reminder fee (defaults to 100 kr). The core registration step
 * enforces the rentel. § 9b limits — a 4th reminder, a fee above 100 kr or
 * a reminder less than 10 days after the previous one is rejected by the
 * core with a clear message that the handler echoes back as a 400.
 *
 * The handler is intentionally compound (3 core calls) but stays a thin
 * shell — every business rule remains in the core functions the CLI's
 * `invoice remind` / `invoice post-reminder` / `invoice send --kind
 * reminder` commands already exercise. The cockpit becomes a third caller
 * of the SAME functions, never a parallel implementation.
 */
export async function handleInvoiceSendReminder(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      void ctx.actor;
      const invoiceDocumentId = requireBodyPositiveInt(body, "invoiceDocumentId");
      const to = requireBodyString(body, "to");
      const bookFee = optionalBodyBoolean(body, "bookFee") ?? true;
      const feeAmount = optionalBodyNumber(body, "feeAmount");

      // (0) Load AND fully validate everything that can block the send BEFORE
      //     any ledger write: the config file must exist, its CONTENT must be
      //     valid (non-empty host, positive port, valid fromAddress — the same
      //     checks sendInvoiceEmail runs internally at step 3), and the
      //     recipient must be a real e-mail. All of these throw
      //     ApiError.badRequest here, so the rentel. § 9b reminder and the
      //     booked fee are never committed for a send that cannot happen.
      //     Without this, the reminder + fee were permanently appended to the
      //     append-only ledger and then a 400 was returned — a phantom reminder
      //     that also counts against the max-3 / 10-days-apart limits on the
      //     next attempt. (Validating only file-existence left the empty-host /
      //     bad-fromAddress / bad-recipient cases still phantom-booking.)
      const smtp = loadConfiguredSmtp(ctx.companyRoot);
      const smtpErrors = validateSmtpConfig(smtp);
      if (smtpErrors.length > 0) {
        throw ApiError.badRequest(smtpErrors[0] ?? "SMTP er ikke konfigureret korrekt");
      }
      if (!looksLikeEmail(to)) {
        throw ApiError.badRequest(
          "modtagerens e-mailadresse er ugyldig — rykkeren kan ikke sendes",
        );
      }
      const transport = createSmtpTransport(smtp);

      // Reminder date is "today" in ISO form (UTC slice) — the core enforces
      // the "10 days since previous reminder" rule and refuses anything that
      // would breach the statutory series.
      const reminderDate = new Date().toISOString().slice(0, 10);

      // (1) Register the reminder + fee. The core enforces:
      //     - the invoice must exist and be an issued_invoice,
      //     - currency must be DKK,
      //     - the invoice must be overdue with a positive open balance,
      //     - fee <= 100 kr,
      //     - <= 3 reminders per invoice,
      //     - >= 10 days since the previous reminder.
      const registered = registerInvoiceReminder(ctx.db, {
        invoiceDocumentId,
        reminderDate,
        feeAmount,
      });
      if (!registered.ok) {
        throw ApiError.badRequest(
          registered.errors[0] ?? "rykkeren kunne ikke registreres",
        );
      }

      // (2) Optionally journal the fee — the standard owner intent. The
      //     core uses the same receivable/income accounts as the CLI's
      //     `invoice post-reminder` command, so the journal entry is
      //     byte-identical no matter which surface the owner uses.
      let journalEntryNo: string | null = null;
      let feeBooked = false;
      if (bookFee) {
        const posted = postInvoiceReminderToLedger(ctx.db, {
          invoiceDocumentId,
          reminderId: registered.reminderId,
        });
        if (!posted.ok) {
          throw ApiError.badRequest(
            posted.errors?.[0] ?? "rykkergebyret kunne ikke bogføres",
          );
        }
        journalEntryNo = posted.entryNo ?? null;
        feeBooked = true;
      }

      // (3) Send the reminder e-mail. SAME core function as "Send på mail"
      //     (#429), with `kind: 'reminder'` so the subject + body match
      //     what `invoice send --kind reminder` produces from the CLI. The
      //     SMTP config + transport were already loaded in step (0).
      const sent = sendInvoiceEmail(ctx.db, ctx.companyRoot, {
        invoiceDocumentId,
        kind: "reminder",
        to,
        smtp,
        transport,
      });
      if (!sent.ok) {
        throw ApiError.badRequest(
          sent.errors?.[0] ?? "rykkermailen kunne ikke sendes",
        );
      }

      return {
        ok: true as const,
        invoiceNumber: registered.invoiceNumber ?? sent.invoiceNumber ?? null,
        recipient: sent.recipient ?? to,
        reminderSequence: registered.reminderSequence ?? null,
        feeAmount: registered.feeAmount ?? null,
        feeBooked,
        journalEntryNo,
        messageId: sent.messageId ?? null,
        duplicate: Boolean(sent.duplicate),
      };
    },
    { requireConfirm: true },
  );

  return okResponse({
    reminder: {
      invoiceNumber: result.invoiceNumber ?? null,
      recipient: result.recipient ?? null,
      reminderSequence: result.reminderSequence ?? null,
      feeAmount: result.feeAmount ?? null,
      feeBooked: Boolean(result.feeBooked),
      journalEntryNo: result.journalEntryNo ?? null,
      messageId: result.messageId ?? null,
      duplicate: Boolean(result.duplicate),
    },
  });
}
