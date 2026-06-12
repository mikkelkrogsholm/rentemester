/**
 * Shared building blocks for the `invoice <subcommand>` CLI handlers.
 *
 * Split out of `../invoice.ts`. Everything here is consumed by 2+ per-sub-domain
 * register helpers (`issuance.ts`, `settlement.ts`, `reminder.ts`,
 * `interest.ts`, `compensation.ts`, `query.ts`). Anything that only one file
 * needs stays inline in that file.
 */

import { openDb } from "../../core/db";
import type { CommandContext } from "../../cli-dispatch";

export type Db = ReturnType<typeof openDb>;

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
export function resolveInvoiceDocumentId(db: Db, ctx: CommandContext): number {
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

export function withResolvedInvoicePayload<T extends Record<string, unknown>>(
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
