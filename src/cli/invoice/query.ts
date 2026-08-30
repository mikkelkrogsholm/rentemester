/**
 * Query-side `invoice <subcommand>` CLI handlers:
 *   status, list, find, overdue.
 *
 * Split out of `../invoice.ts`. Registration order preserved.
 */

import { openCommandDb, optionalNumberOrFatal } from "../../cli-dispatch";
import { migrate } from "../../core/db";
import { getInvoiceStatus } from "../../core/invoice-payments";
import { buildInvoiceList, buildOverdueInvoiceList, findInvoices } from "../../core/invoice-list";
import { listImportedReceivables } from "../../core/imported-receivables";
import { invoiceStatusDa } from "../../core/messages";
import type { CommandDispatch } from "../../cli-dispatch";
import { emitHumanReport, formatKroner } from "../../cli-format";
import { resolveInvoiceDocumentId } from "./_shared";

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

export function registerQueryCommands(dispatch: CommandDispatch): void {
  dispatch.on("invoice", "imported-receivables", (ctx) => {
    const db = openCommandDb(ctx); migrate(db);
    const result = listImportedReceivables(db, ctx.arg("--as-of") ?? new Date().toISOString().slice(0,10));
    ctx.emitResult(result as Record<string, unknown>); db.close();
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
    const minAmount = optionalNumberOrFatal(ctx, "--min-amount");
    const maxAmount = optionalNumberOrFatal(ctx, "--max-amount");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildInvoiceList(db, {
      status: (ctx.arg("--status") as any) ?? "all",
      from: ctx.arg("--from") ?? undefined,
      to: ctx.arg("--to") ?? undefined,
      customerCvr: ctx.arg("--customer-cvr") ?? undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount,
      maxAmount,
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
    const amount = optionalNumberOrFatal(ctx, "--amount");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = findInvoices(db, {
      query: ctx.parsedArgs.positionals.slice(2).join(" ") || undefined,
      customer: ctx.arg("--customer") ?? undefined,
      invoiceNumber: ctx.arg("--invoice-number") ?? undefined,
      minAmount: amount,
      maxAmount: amount,
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
    const minDays = optionalNumberOrFatal(ctx, "--min-days");
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildOverdueInvoiceList(db, {
      asOfDate: ctx.arg("--as-of") ?? undefined,
      minDays,
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
}
