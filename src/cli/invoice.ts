import { readFileSync } from "node:fs";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import {
  validateInvoice,
  computeInvoiceAmounts,
  type InvoiceLineInput,
  type InvoicePayload,
} from "../core/invoice";
import { issueInvoice } from "../core/issued-invoices";
import { renderIssuedInvoicePdf } from "../core/invoice-pdf";
import {
  exportPublicEInvoiceOioUbl,
  exportPublicEInvoicePreview,
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
  type PeppolTransportAcknowledgement,
} from "../core/public-einvoice";
import { applyInvoicePayment, getInvoiceStatus } from "../core/invoice-payments";
import { buildInvoiceList, buildOverdueInvoiceList, findInvoices } from "../core/invoice-list";
import { invoiceStatusDa } from "../core/messages";
import { postIssuedInvoiceToLedger } from "../core/invoice-booking";
import { settleInvoiceFromBank } from "../core/invoice-settlement";
import { settleInvoiceClaimsFromBank } from "../core/invoice-claim-settlement";
import { issueCreditNote } from "../core/credit-notes";
import { refundInvoiceToBank } from "../core/invoice-refunds";
import {
  calculateInvoiceLateInterest,
  postInvoiceLateInterestToLedger,
  registerInvoiceLateInterest,
} from "../core/invoice-interest";
import {
  calculateInvoiceLateCompensation,
  postInvoiceLateCompensationToLedger,
  registerInvoiceLateCompensation,
} from "../core/invoice-compensation";
import { postInvoiceReminderToLedger, registerInvoiceReminder } from "../core/invoice-reminders";
import { writeOffInvoiceBadDebt } from "../core/invoice-bad-debt";
import { resolveInvoiceMasterData } from "../core/master-data";
import { openCommandDb } from "../cli-dispatch";
import type { CommandDispatch, CommandContext } from "../cli-dispatch";
import { emitHumanReport, emitHumanWrite, formatKroner } from "../cli-format";

type Db = ReturnType<typeof openDb>;

// Resolving an invoice from either --document-id or --invoice-number has three
// distinct outcomes, and #230 is that the CLI used to collapse the last two:
//   - "found"      : a flag was given and it resolved to an invoice document.
//   - "no-flag"    : neither --document-id nor --invoice-number was supplied.
//                    This is a USAGE error (exit 2 — fix the call).
//   - "not-found"  : a flag WAS supplied but matched no invoice. This is a
//                    BUSINESS error (exit 1 — the call was well-formed, the
//                    invoice simply does not exist), so retrying is pointless.
type InvoiceResolution =
  | { kind: "found"; documentId: number }
  | { kind: "no-flag" }
  | { kind: "not-found"; flag: "--document-id" | "--invoice-number"; value: string };

function resolveInvoiceDocument(db: Db, ctx: CommandContext): InvoiceResolution {
  const documentIdRaw = ctx.arg("--document-id");
  if (documentIdRaw !== undefined) {
    const documentId = Number(documentIdRaw);
    if (Number.isInteger(documentId) && documentId > 0) {
      const row = db
        .query(`SELECT id FROM documents WHERE document_type = 'issued_invoice' AND id = ? LIMIT 1`)
        .get(documentId) as { id: number } | null;
      if (row) return { kind: "found", documentId: row.id };
    }
    return { kind: "not-found", flag: "--document-id", value: String(documentIdRaw) };
  }
  const invoiceNumber = ctx.arg("--invoice-number")?.trim();
  if (invoiceNumber) {
    const row = db
      .query(
        `SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`,
      )
      .get(invoiceNumber) as { id: number } | null;
    if (row) return { kind: "found", documentId: row.id };
    return { kind: "not-found", flag: "--invoice-number", value: invoiceNumber };
  }
  return { kind: "no-flag" };
}

/**
 * Resolve the invoice document id for a command, or emit the right error and
 * exit. Distinguishes a missing identifying flag (usage error, exit 2) from a
 * supplied-but-unmatched id/number (business error, exit 1). (#230)
 */
function resolveInvoiceDocumentId(db: Db, ctx: CommandContext): number {
  const resolution = resolveInvoiceDocument(db, ctx);
  if (resolution.kind === "found") return resolution.documentId;
  if (resolution.kind === "no-flag") {
    console.error("Missing required --document-id <n> or --invoice-number <no>");
    db.close();
    process.exit(2);
  }
  // not-found: the call was well-formed; the invoice simply does not exist.
  const label =
    resolution.flag === "--invoice-number"
      ? `No issued invoice has invoice number ${resolution.value}`
      : `No issued invoice has document id ${resolution.value}`;
  ctx.emitResult({
    ok: false,
    appliedRules: [],
    errors: [`${label} — check the value with 'invoice list' or 'invoice find'`],
  });
  db.close();
  process.exit(1);
}

function findInvoiceDocumentIdByNumber(db: Db, invoiceNumber?: string | null): number | null {
  const value = invoiceNumber?.trim();
  if (!value) return null;
  const row = db
    .query(
      `SELECT id FROM documents WHERE document_type = 'issued_invoice' AND invoice_no = ? LIMIT 1`,
    )
    .get(value) as { id: number } | null;
  return row?.id ?? null;
}

function withResolvedInvoicePayload<T extends Record<string, unknown>>(
  db: Db,
  payload: T,
  idKey: keyof T,
  numberKey: keyof T,
): T {
  if (Number.isInteger(payload[idKey] as number) && Number(payload[idKey]) > 0) return payload;
  const resolved = findInvoiceDocumentIdByNumber(db, payload[numberKey] as string | undefined);
  if (!resolved) return payload;
  return { ...payload, [idKey]: resolved };
}

function renderInvoiceRowsHuman(title: string, rows: any[], emptyMessage: string): void {
  console.log(title);
  if (rows.length === 0) {
    console.log(emptyMessage);
    return;
  }
  for (const row of rows) {
    const statusDa = row.status != null ? invoiceStatusDa(String(row.status)) : "—";
    const customer = row.customerName ?? "(ukendt kunde)";
    console.log("");
    console.log(`Faktura ${row.invoiceNumber} — ${customer}`);
    console.log(
      `  Fakturadato: ${row.invoiceDate ?? "—"} | Forfald: ${row.effectiveDueDate ?? "—"}`,
    );
    console.log(
      `  Beløb (inkl. moms): ${formatKroner(row.grossAmount)} | Åben saldo: ${formatKroner(row.openBalance)}`,
    );
    const overdue =
      typeof row.overdueDays === "number" && row.overdueDays > 0
        ? ` | ${row.overdueDays} dage forfalden`
        : "";
    console.log(`  Status: ${statusDa}${overdue}`);
  }
}

// `invoice create` line parsing (#212): a human enters one or more lines as a
// single delimited string instead of hand-writing JSON. Lines are separated by
// `;`, fields within a line by `|` — "description|quantity|unitPrice". Only the
// three essentials are accepted; Rentemester computes every total.
type ParsedLines = { ok: true; lines: InvoiceLineInput[] } | { ok: false; errors: string[] };

function parseHumanInvoiceLines(raw: string | undefined): ParsedLines {
  const errors: string[] = [];
  const segments = (raw ?? "")
    .split(";")
    .map((segment) => segment.trim())
    .filter((segment) => segment.length > 0);
  if (segments.length === 0) {
    return { ok: false, errors: ["at least one --line \"description|quantity|unitPrice\" is required"] };
  }
  const lines: InvoiceLineInput[] = [];
  segments.forEach((segment, index) => {
    const parts = segment.split("|").map((part) => part.trim());
    if (parts.length !== 3) {
      errors.push(
        `line ${index + 1} (\"${segment}\") must have exactly 3 fields: description|quantity|unitPrice`,
      );
      return;
    }
    const [description, quantityText, unitPriceText] = parts as [string, string, string];
    const quantity = Number(quantityText);
    const unitPriceExVat = Number(unitPriceText);
    if (quantityText === "" || !Number.isFinite(quantity)) {
      errors.push(`line ${index + 1} quantity \"${quantityText}\" must be a number`);
    }
    if (unitPriceText === "" || !Number.isFinite(unitPriceExVat)) {
      errors.push(`line ${index + 1} unit price \"${unitPriceText}\" must be a number`);
    }
    lines.push({ description, quantity, unitPriceExVat });
  });
  if (errors.length > 0) return { ok: false, errors };
  return { ok: true, lines };
}

export function register(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "validate", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const result = validateInvoice(payload);
    // #250: render the validation verdict — valid/invalid plus every concrete
    // rejection reason — in Danish for a human; `--format json` is unchanged.
    emitHumanReport("invoice-validate", result as Record<string, unknown>, ctx.outputFormat);
  });

  dispatch.on("invoice", "issue", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const customerIdRaw = ctx.arg("--customer-id");
    const customerId = customerIdRaw === undefined ? undefined : Number(customerIdRaw);
    const resolved = resolveInvoiceMasterData(db, payload, {
      customerId: Number.isInteger(customerId) && customerId > 0 ? customerId : undefined,
    });
    if (!resolved.ok) {
      ctx.emitResult(resolved as Record<string, unknown>);
      db.close();
      process.exit(1);
    }
    const result = issueInvoice(db, root, resolved.payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  // invoice create (#212): the guided path. A human supplies only the
  // essentials — seller, buyer/customer, dates and one or more lines
  // (description, quantity, unit price ex-VAT) plus a single VAT rate.
  // Rentemester computes every line total, the net amount, the VAT amount and
  // the gross amount, expands a full payload, then validates and issues it
  // through the SAME core as `invoice issue`. The human never does invoice
  // arithmetic and never hand-writes a JSON payload.
  dispatch.on("invoice", "create", (ctx) => {
    const issueDate = ctx.arg("--issue-date");
    if (!issueDate) {
      console.error("Missing required --issue-date <YYYY-MM-DD>");
      process.exit(2);
    }

    const parsedLines = parseHumanInvoiceLines(ctx.arg("--line"));
    if (!parsedLines.ok) {
      ctx.emitResult({ ok: false, appliedRules: [], errors: parsedLines.errors });
      process.exit(1);
    }

    const vatRateArg = ctx.arg("--vat-rate", "25");
    const vatRatePercent = Number(vatRateArg);
    if (!Number.isFinite(vatRatePercent)) {
      console.error("--vat-rate must be a number (percent, e.g. 25)");
      process.exit(2);
    }

    const computed = computeInvoiceAmounts(parsedLines.lines, vatRatePercent);
    if (!computed.ok) {
      ctx.emitResult({ ok: false, appliedRules: [], errors: computed.errors });
      process.exit(1);
    }

    const customerIdRaw = ctx.arg("--customer-id");
    const customerId = customerIdRaw === undefined ? undefined : Number(customerIdRaw);
    const hasCustomerRow = Number.isInteger(customerId) && (customerId as number) > 0;

    // The human gives buyer details either via a stored --customer-id (master
    // data fills name/address) or by typing them directly. Without either,
    // validation will reject the invoice with a clear message.
    const buyer: InvoicePayload["buyer"] = {};
    const buyerName = ctx.trimToNull(ctx.arg("--buyer-name") ?? null);
    const buyerAddress = ctx.trimToNull(ctx.arg("--buyer-address") ?? null);
    const buyerVat = ctx.trimToNull(ctx.arg("--buyer-vat") ?? null);
    if (buyerName) buyer.name = buyerName;
    if (buyerAddress) buyer.address = buyerAddress;
    if (buyerVat) buyer.vatOrCvr = buyerVat;

    const payload: InvoicePayload = {
      invoiceType: "full",
      vatTreatment: "standard",
      issueDate,
      ...(ctx.trimToNull(ctx.arg("--invoice-number") ?? null)
        ? { invoiceNumber: ctx.trimToNull(ctx.arg("--invoice-number") ?? null)! }
        : {}),
      seller: {
        name: ctx.trimToNull(ctx.arg("--seller-name") ?? null) ?? undefined,
        address: ctx.trimToNull(ctx.arg("--seller-address") ?? null) ?? undefined,
        vatOrCvr: ctx.trimToNull(ctx.arg("--seller-vat") ?? null) ?? undefined,
      },
      buyer,
      lines: computed.lines,
      totals: {
        netAmount: computed.totals.netAmount,
        vatRate: computed.totals.vatRate,
        vatAmount: computed.totals.vatAmount,
        grossAmount: computed.totals.grossAmount,
      },
      currency: ctx.trimToNull(ctx.arg("--currency") ?? null) ?? "DKK",
      ...(ctx.trimToNull(ctx.arg("--due-date") ?? null)
        ? { dueDate: ctx.trimToNull(ctx.arg("--due-date") ?? null)! }
        : {}),
    };

    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const resolved = resolveInvoiceMasterData(db, payload, {
      customerId: hasCustomerRow ? customerId : undefined,
    });
    if (!resolved.ok) {
      ctx.emitResult(resolved as Record<string, unknown>);
      db.close();
      process.exit(1);
    }
    const result = issueInvoice(db, root, resolved.payload);
    // Surface the computed amounts so a human can see exactly what Rentemester
    // worked out from their input. The core result fields stay untouched, so
    // `--format json` consumers keep a stable shape.
    const enriched = {
      ...result,
      computed: {
        lines: computed.lines,
        netAmount: computed.totals.netAmount,
        vatRate: computed.totals.vatRate,
        vatRatePercent,
        vatAmount: computed.totals.vatAmount,
        grossAmount: computed.totals.grossAmount,
      },
    };
    // #268: write commands had useless `--format human` output (the command
    // description as heading, English labels, no figures). #266: the output
    // never said the invoice still needs `invoice post`. `emitHumanWrite`
    // renders both in Danish; `--format json` stays byte-identical.
    emitHumanWrite("invoice-create", enriched as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "render", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = renderIssuedInvoicePdf(db, ctx.companyRoot(), { invoiceDocumentId: documentId });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "export-public", (ctx) => {
    const out = ctx.arg("--out");
    if (!out) {
      console.error("Missing required --out <file.xml>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = exportPublicEInvoicePreview(db, {
      invoiceDocumentId: documentId,
      outPath: out,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("invoice", "export-public-oioubl", (ctx) => {
    const out = ctx.arg("--out");
    if (!out) {
      console.error("Missing required --out <file.xml>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = exportPublicEInvoiceOioUbl(db, {
      invoiceDocumentId: documentId,
      outPath: out,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  // PEPPOL submission (#128): builds a deterministic submission envelope on
  // top of the OIOUBL handoff artifact. Access-point config is read from a
  // file (--access-point) so credentials never enter core bookkeeping state.
  dispatch.on("invoice", "submit-public-peppol", (ctx) => {
    const accessPointPath = ctx.arg("--access-point");
    if (!accessPointPath) {
      console.error("Missing required --access-point <file.json>");
      process.exit(2);
    }
    let accessPoint: PeppolAccessPointConfig;
    let acknowledgement: PeppolTransportAcknowledgement | undefined;
    try {
      const parsed = JSON.parse(readFileSync(accessPointPath, "utf8")) as {
        accessPointId?: string;
        endpointUrl?: string;
        senderEndpointId?: string;
        acknowledgement?: PeppolTransportAcknowledgement;
      };
      accessPoint = {
        accessPointId: parsed.accessPointId ?? "",
        endpointUrl: parsed.endpointUrl ?? "",
        senderEndpointId: parsed.senderEndpointId ?? "",
      };
      acknowledgement = parsed.acknowledgement;
    } catch (error) {
      console.error(`Could not read --access-point config: ${(error as Error).message}`);
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = submitPublicEInvoicePeppol(db, {
      invoiceDocumentId: documentId,
      accessPoint,
      acknowledgement,
      outPath: ctx.arg("--out"),
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
    if (!result.ok) process.exit(1);
  });

  dispatch.on("invoice", "credit-note", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "originalInvoiceDocumentId",
      "originalInvoiceNumber",
    );
    const result = issueCreditNote(db, root, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postIssuedInvoiceToLedger(db, { invoiceDocumentId: documentId });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "settle-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = settleInvoiceFromBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "settle-claim-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = settleInvoiceClaimsFromBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "write-off-bad-debt", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = writeOffInvoiceBadDebt(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "apply-payment", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = applyInvoicePayment(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "refund-bank", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = withResolvedInvoicePayload(
      db,
      JSON.parse(readFileSync(input, "utf8")),
      "invoiceDocumentId",
      "invoiceNumber",
    );
    const result = refundInvoiceToBank(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "remind", (ctx) => {
    const reminderDate = ctx.arg("--date");
    const feeArg = ctx.arg("--fee");
    const feeAmount = feeArg === undefined ? undefined : Number(feeArg);
    const note = ctx.arg("--note");
    if (!reminderDate || (feeArg !== undefined && Number.isNaN(feeAmount))) {
      console.error(
        "Missing required --date <YYYY-MM-DD>; optional --fee <n> must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceReminder(db, {
      invoiceDocumentId: documentId,
      reminderDate,
      feeAmount,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-reminder", (ctx) => {
    const reminderIdArg = ctx.arg("--reminder-id");
    const reminderId = reminderIdArg === undefined ? undefined : Number(reminderIdArg);
    const transactionDate = ctx.arg("--date");
    if (
      reminderIdArg !== undefined &&
      (!Number.isInteger(reminderId) || (reminderId as number) <= 0)
    ) {
      console.error("Optional --reminder-id <n> must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceReminderToLedger(db, {
      invoiceDocumentId: documentId,
      reminderId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "status", (ctx) => {
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = getInvoiceStatus(db, documentId, ctx.arg("--as-of"));
    emitHumanReport("invoice-status", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "list", (ctx) => {
    const minAmount = ctx.parseOptionalNumber("--min-amount");
    const maxAmount = ctx.parseOptionalNumber("--max-amount");
    if (!minAmount.ok) ctx.fatal(minAmount.error);
    if (!maxAmount.ok) ctx.fatal(maxAmount.error);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildInvoiceList(db, {
      status: (ctx.arg("--status") as any) ?? "all",
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      customerCvr: ctx.arg("--customer-cvr") ?? undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount: minAmount.value,
      maxAmount: maxAmount.value,
      asOfDate: ctx.arg("--as-of") ?? undefined,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(
        `Fakturaer (${result.count})`,
        result.rows,
        "Ingen fakturaer for det valgte filter.",
      );
    }
    db.close();
  });

  dispatch.on("invoice", "find", (ctx) => {
    const amount = ctx.parseOptionalNumber("--amount");
    if (!amount.ok) ctx.fatal(amount.error);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = findInvoices(db, {
      query: ctx.parsedArgs.positionals.slice(2).join(" ") || undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount: amount.value,
      maxAmount: amount.value,
      asOfDate: ctx.arg("--as-of") ?? undefined,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(
        `Fakturatræf (${result.count})`,
        result.rows,
        "Ingen fakturaer matchede søgningen.",
      );
    }
    db.close();
  });

  dispatch.on("invoice", "overdue", (ctx) => {
    const minDays = ctx.parseOptionalNumber("--min-days");
    if (!minDays.ok) ctx.fatal(minDays.error);
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildOverdueInvoiceList(db, {
      asOfDate: ctx.arg("--as-of") ?? undefined,
      minDays: minDays.value,
    });
    if (ctx.outputFormat === "json") {
      ctx.emitResult(result as Record<string, unknown>);
    } else {
      renderInvoiceRowsHuman(
        `Forfaldne fakturaer pr. ${result.asOfDate ?? "i dag"} (${result.count})`,
        result.rows,
        "Ingen forfaldne fakturaer for det valgte filter.",
      );
    }
    db.close();
  });

  dispatch.on("invoice", "interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const referenceRatePercent = Number(ctx.arg("--reference-rate"));
    if (!asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error("Missing required --as-of <YYYY-MM-DD> or --reference-rate <pct>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = calculateInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
    });
    // #250: render the late-interest figures (reference rate, statutory annual
    // rate, overdue window, computed amount) in Danish; `--format json` is
    // unchanged.
    emitHumanReport("invoice-interest", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "claim-interest", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const rateArg = ctx.arg("--reference-rate");
    const note = ctx.arg("--note");
    const referenceRatePercent = rateArg === undefined ? NaN : Number(rateArg);
    if (!asOfDate || Number.isNaN(referenceRatePercent)) {
      console.error("Missing required --as-of <YYYY-MM-DD> or --reference-rate <pct>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceLateInterest(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      referenceRatePercent,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-interest", (ctx) => {
    const claimIdArg = ctx.arg("--claim-id");
    const claimId = claimIdArg === undefined ? undefined : Number(claimIdArg);
    const transactionDate = ctx.arg("--date");
    if (
      claimIdArg !== undefined &&
      (!Number.isInteger(claimId) || (claimId as number) <= 0)
    ) {
      console.error("Optional --claim-id <n> must be a positive integer when present");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceLateInterestToLedger(db, {
      invoiceDocumentId: documentId,
      claimId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    if (!asOfDate || (amountArg !== undefined && Number.isNaN(compensationAmountDkk))) {
      console.error(
        "Missing required --as-of <YYYY-MM-DD>; optional --amount-dkk <n> must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = calculateInvoiceLateCompensation(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      compensationAmountDkk,
    });
    // #250: render the compensation assessment — eligibility, amount and a
    // clear reason — in Danish; `--format json` is unchanged.
    emitHumanReport("invoice-compensation", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("invoice", "claim-compensation", (ctx) => {
    const asOfDate = ctx.arg("--as-of");
    const amountArg = ctx.arg("--amount-dkk");
    const note = ctx.arg("--note");
    const compensationAmountDkk = amountArg === undefined ? undefined : Number(amountArg);
    if (!asOfDate || (amountArg !== undefined && Number.isNaN(compensationAmountDkk))) {
      console.error(
        "Missing required --as-of <YYYY-MM-DD>; optional --amount-dkk must be numeric when present",
      );
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = registerInvoiceLateCompensation(db, {
      invoiceDocumentId: documentId,
      asOfDate,
      compensationAmountDkk,
      note: note ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("invoice", "post-compensation", (ctx) => {
    const transactionDate = ctx.arg("--date");
    const db = openCommandDb(ctx);
    migrate(db);
    const documentId = resolveInvoiceDocumentId(db, ctx);
    const result = postInvoiceLateCompensationToLedger(db, {
      invoiceDocumentId: documentId,
      transactionDate: transactionDate ?? undefined,
    });
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
