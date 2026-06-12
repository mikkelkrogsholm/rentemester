/**
 * Issuance-side `invoice <subcommand>` CLI handlers:
 *   validate, issue, create, render, export-public, export-public-oioubl,
 *   submit-public-peppol, credit-note, post.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { readJsonCliInput } from "../../cli-dispatch";
import { companyPaths } from "../../core/paths";
import { openDb, migrate } from "../../core/db";
import {
  validateInvoice,
  computeInvoiceAmounts,
  type InvoiceLineInput,
  type InvoicePayload,
} from "../../core/invoice";
import { issueInvoice } from "../../core/issued-invoices";
import { renderIssuedInvoicePdf } from "../../core/invoice-pdf";
import {
  exportPublicEInvoiceOioUbl,
  exportPublicEInvoicePreview,
  submitPublicEInvoicePeppol,
  type PeppolAccessPointConfig,
  type PeppolTransportAcknowledgement,
} from "../../core/public-einvoice";
import { issueCreditNote } from "../../core/credit-notes";
import { postIssuedInvoiceToLedger } from "../../core/invoice-booking";
import { resolveInvoiceMasterData } from "../../core/master-data";
import { openCommandDb } from "../../cli-dispatch";
import type { CommandDispatch } from "../../cli-dispatch";
import { emitHumanReport, emitHumanWrite } from "../../cli-format";
import { resolveInvoiceDocumentId, withResolvedInvoicePayload } from "./_shared";

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

export function registerIssuanceCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "validate", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const payload = readJsonCliInput(ctx, input, "--input");
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
    const payload = readJsonCliInput(ctx, input, "--input");
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
    const parsed = readJsonCliInput(ctx, accessPointPath, "--access-point") as {
      accessPointId?: string;
      endpointUrl?: string;
      senderEndpointId?: string;
      acknowledgement?: PeppolTransportAcknowledgement;
    };
    const accessPoint: PeppolAccessPointConfig = {
      accessPointId: parsed.accessPointId ?? "",
      endpointUrl: parsed.endpointUrl ?? "",
      senderEndpointId: parsed.senderEndpointId ?? "",
    };
    const acknowledgement: PeppolTransportAcknowledgement | undefined = parsed.acknowledgement;
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
      readJsonCliInput(ctx, input, "--input"),
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
}
