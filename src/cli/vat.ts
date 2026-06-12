import { readFileSync } from "node:fs";
import { migrate } from "../core/db";
import {
  buildVatReport,
  postEuServiceReverseChargePurchase,
  postRepresentationPurchase,
} from "../core/vat";
import { openCommandDb } from "../cli-dispatch";
import type { CommandContext, CommandDispatch } from "../cli-dispatch";
import { emitHumanReport } from "../cli-format";
// ===== VAT FILING (#178) =====
import { buildVatFiling } from "../core/vat-filing";
// ===== END VAT FILING (#178) =====
// ===== EU SALES LIST + OSS =====
import { buildViesRecapitulativeStatement } from "../core/vat-vies-list";
import { buildOssReport } from "../core/vat-oss";
// ===== END EU SALES LIST + OSS =====

export function register(dispatch: CommandDispatch): void {
  dispatch.on("vat", "report", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildVatReport(db, from, to);
    emitHumanReport("vat-report", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  });

  dispatch.on("vat", "post-eu-service-purchase", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const invoiceNo = typeof payload.invoiceNo === "string" ? payload.invoiceNo.trim() : "";
    if (invoiceNo) {
      const row = db
        .query(`SELECT id FROM documents WHERE invoice_no = ? ORDER BY id DESC LIMIT 1`)
        .get(invoiceNo) as { id: number } | null;
      if (!row) {
        console.error(`Could not resolve document for invoiceNo ${invoiceNo}`);
        process.exit(2);
      }
      payload.documentId = row.id;
    }
    const result = postEuServiceReverseChargePurchase(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("vat", "post-representation-purchase", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const result = postRepresentationPurchase(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  // ===== VAT FILING (#178) =====
  // `vat momsangivelse` (alias `vat filing`): produce a filing-ready
  // momsangivelse — the standard SKAT rubrikker plus momstilsvar — for a
  // closed VAT period. Read-only: Rentemester produces the numbers, the user
  // files them via TastSelv.
  const emitVatFiling = (ctx: CommandContext) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildVatFiling(db, from, to);
    emitHumanReport("vat-filing", result as Record<string, unknown>, ctx.outputFormat);
    db.close();
  };
  dispatch.on("vat", "momsangivelse", emitVatFiling);
  dispatch.on("vat", "filing", emitVatFiling);
  // ===== END VAT FILING (#178) =====

  // ===== EU SALES LIST + OSS =====
  // `vat eu-sales-list`: produce the EU-salg uden moms-liste (VIES
  // recapitulative statement) — a per-customer listing of cross-border B2B
  // sales without Danish VAT for the period. This is a separate mandatory
  // filing, distinct from the momsangivelse. Read-only.
  dispatch.on("vat", "eu-sales-list", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildViesRecapitulativeStatement(db, from, to);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  // `vat oss-report`: a deterministic OSS (One Stop Shop) first-slice report
  // — the OSS consumer-sales base for the period. Not an OSS filing to SKAT.
  dispatch.on("vat", "oss-report", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openCommandDb(ctx);
    migrate(db);
    const result = buildOssReport(db, from, to);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
  // ===== END EU SALES LIST + OSS =====
}
