// Cockpit data-import + accountant-export handlers.

import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { importDineroContacts } from "../../core/import/dinero-contacts";
import { detectImportSource } from "../../core/import/source-detect";
import { exportAuthorityPackage } from "../../core/authority-export";
import { createTar, dirToTarEntries } from "../../core/tar";
import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import { withCompanyMutation } from "../mutations";
import {
  MAX_UPLOAD_BODY_BYTES,
  okResponse,
  requireBodyString,
} from "./_shared";

/**
 * POST /api/companies/:slug/import — the cockpit's generic file-import.
 *
 * Body: `{ fileName: string, content: string, enrichCvr?: boolean,
 * confirm: true }`. The browser reads the chosen export file and POSTs its
 * text; the handler recognises WHICH system the file came from
 * (`detectImportSource`) and routes it to the matching core importer. Today
 * one source is recognised — a Dinero "Kontakter" CSV, landed in the
 * customer/vendor master data via the same `importDineroContacts` core the
 * CLI's `import contacts` uses.
 *
 * A write (it appends master-data rows) so `requireConfirm` is set; the upload
 * route is capped by `maxBodyBytes`. Goes through `withCompanyMutation`, so the
 * backup lock, the localhost gate and actor attribution all apply. A file that
 * matches no known format is a 400 with the supported-formats list.
 */
export async function handleDataImport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    async (ctx, body) => {
      const fileName = requireBodyString(body, "fileName");
      const content = requireBodyString(body, "content");
      const enrichCvr = body.enrichCvr === true;

      const detection = detectImportSource(fileName, content);
      if (!detection.ok) {
        throw ApiError.badRequest(detection.errors.join(" "));
      }
      const source = detection.module;

      if (source.dataType === "contacts") {
        const imported = await importDineroContacts(ctx.db, content, {
          enrichCvr,
        });
        return {
          ok: imported.ok,
          errors: imported.errors,
          detected: {
            id: source.id,
            label: source.label,
            system: source.system,
            dataType: source.dataType,
          },
          summary: imported.summary,
        };
      }

      // Unreachable today — every registered module's dataType is "contacts".
      throw ApiError.badRequest(
        `Datatypen '${source.dataType}' understøttes ikke endnu.`,
      );
    },
    { requireConfirm: true, maxBodyBytes: MAX_UPLOAD_BODY_BYTES },
  );

  return okResponse({
    import: {
      detected: result.detected,
      summary: result.summary,
      errors: result.errors,
    },
  });
}

/**
 * POST /api/companies/:slug/accountant-export — the "share with revisor"
 * action.
 *
 * Body: `{ periodStart: string, periodEnd: string, confirm: true }`. Generates
 * the same `accountant_handoff` package the CLI's `system export-accountant`
 * produces (a manifest plus the machine-readable + documents-readable
 * subtrees), packs the whole thing into one deterministic .tar, and returns
 * the archive as a single download. The temp output dir is removed on the way
 * out — the response is the only copy that leaves the workspace.
 *
 * Goes through `withCompanyMutation` (backup lock, localhost gate, actor
 * attribution); `requireConfirm` is set because the export writes an audit
 * event into the ledger.
 */
export async function handleAccountantExport(
  config: ServerConfig,
  request: Request,
  slug: string,
): Promise<Response> {
  const result = await withCompanyMutation(
    request,
    config,
    slug,
    (ctx, body) => {
      const periodStart = requireBodyString(body, "periodStart");
      const periodEnd = requireBodyString(body, "periodEnd");

      const outputDir = mkdtempSync(
        join(tmpdir(), "rentemester-cockpit-accountant-"),
      );
      try {
        const exported = exportAuthorityPackage(ctx.db, ctx.companyRoot, {
          periodStart,
          periodEnd,
          outputDir,
          packageProfile: "accountant_handoff",
        });
        if (!exported.ok || !exported.exportDir) {
          throw ApiError.badRequest(
            (exported.errors ?? []).join("; ") ||
              "revisor-eksport mislykkedes",
          );
        }
        // The flat directory the export wrote — packing this (not the parent
        // temp dir) keeps the tar coherent: untarring it yields a single
        // package folder, not a wrapper.
        const entries = dirToTarEntries(exported.exportDir);
        const tar = createTar(entries);
        return {
          ok: true,
          errors: [] as string[],
          tar,
          filename: `revisor-eksport-${slug}-${periodStart}-${periodEnd}.tar`,
          journalEntryCount: exported.journalEntryCount ?? 0,
          documentCount: exported.documentCount ?? 0,
          bankTransactionCount: exported.bankTransactionCount ?? 0,
        };
      } finally {
        try {
          rmSync(outputDir, { recursive: true, force: true });
        } catch {}
      }
    },
    { requireConfirm: true },
  );

  return new Response(result.tar, {
    headers: {
      "content-type": "application/x-tar",
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(result.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
      // Summary counters carried as headers so the UI can show a receipt
      // alongside the download without a second round-trip.
      "x-rentemester-journal-entries": String(result.journalEntryCount),
      "x-rentemester-documents": String(result.documentCount),
      "x-rentemester-bank-transactions": String(result.bankTransactionCount),
    },
  });
}
