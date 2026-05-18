import { readFileSync } from "node:fs";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import {
  buildVatReport,
  postEuServiceReverseChargePurchase,
  postRepresentationPurchase,
} from "../core/vat";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("vat", "report", (ctx) => {
    const from = ctx.arg("--from");
    const to = ctx.arg("--to");
    if (!from || !to) {
      console.error("Missing required --from <YYYY-MM-DD> or --to <YYYY-MM-DD>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const result = buildVatReport(db, from, to);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("vat", "post-eu-service-purchase", (ctx) => {
    const input = ctx.arg("--input");
    if (!input) {
      console.error("Missing required --input <file.json>");
      process.exit(2);
    }
    const db = openDb(companyPaths(ctx.companyRoot()).db);
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
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const payload = JSON.parse(readFileSync(input, "utf8"));
    const result = postRepresentationPurchase(db, payload);
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });
}
