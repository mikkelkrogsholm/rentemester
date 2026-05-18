import { readFileSync } from "node:fs";
import { companyPaths } from "../core/paths";
import { openDb, migrate } from "../core/db";
import { ingestDocument } from "../core/documents";
import { recordException } from "../core/exceptions";
import { resolveDocumentMasterData } from "../core/master-data";
import type { CommandDispatch } from "../cli-dispatch";

export function register(dispatch: CommandDispatch): void {
  dispatch.on("documents", "ingest", (ctx) => {
    const file = ctx.arg("--file");
    const metadataFile = ctx.arg("--metadata");
    if (!file || !metadataFile) {
      console.error("Missing required --file <path> or --metadata <file.json>");
      process.exit(2);
    }
    const root = ctx.companyRoot();
    const db = openDb(companyPaths(root).db);
    migrate(db);
    const metadata = JSON.parse(readFileSync(metadataFile, "utf8"));
    const vendorIdRaw = ctx.arg("--vendor-id");
    const vendorId = vendorIdRaw === undefined ? undefined : Number(vendorIdRaw);
    const resolved = resolveDocumentMasterData(db, metadata, {
      vendorId: Number.isInteger(vendorId) && vendorId > 0 ? vendorId : undefined,
    });
    if (!resolved.ok) {
      ctx.emitResult(resolved as Record<string, unknown>);
      db.close();
      process.exit(1);
    }
    const result = ingestDocument(db, root, file, resolved.metadata, {
      forceDuplicateLogicalIdentity: ctx.hasFlag("--force"),
    });
    if (!result.ok) {
      recordException(db, {
        type: "DOCUMENT_INGEST_BLOCKED",
        severity: "medium",
        message: `Document ingest blocked for ${file}`,
        requiredAction: "Fix document metadata or duplicate handling, then retry ingest.",
        sourceEvidence: {
          file,
          metadataFile,
          errors: result.errors ?? [],
        },
        postingPreview: {
          retryCommand:
            "documents ingest --company <path> --file <file> --metadata <file.json>",
        },
      });
    }
    ctx.emitResult(result as Record<string, unknown>);
    db.close();
  });

  dispatch.on("documents", "list", (ctx) => {
    const db = openDb(companyPaths(ctx.companyRoot()).db);
    migrate(db);
    const rows = db
      .query(
        "SELECT id, document_no, source, original_filename, invoice_date, amount_inc_vat, currency, status, stored_path FROM documents ORDER BY id DESC",
      )
      .all();
    console.table(rows);
    db.close();
  });
}
