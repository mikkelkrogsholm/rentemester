// Invoice list, recurring invoices, and issued-invoice PDF read handlers.

import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import {
  buildCompanyInvoices,
  buildCompanyRecurringInvoices,
  resolveCompanyIssuedInvoicePdf,
  resolveYearParam,
} from "../data";
import { okResponse } from "./_shared";

export function handleCompanyRecurringInvoices(
  config: ServerConfig,
  slug: string,
): Response {
  const data = buildCompanyRecurringInvoices(config.workspaceRoot, slug);
  return okResponse({ recurringInvoices: data });
}

/**
 * GET /api/companies/:slug/invoices/:id/pdf — serves the issued-invoice PDF so
 * the owner can download or forward it without leaving the cockpit (#378). The
 * bytes come from the same `renderIssuedInvoicePdf` core the CLI uses, so the
 * PDF is byte-identical to `bun run cli invoice render <id>`.
 */
export function handleCompanyInvoicePdf(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("invoice id must be a positive integer");
  }
  const file = resolveCompanyIssuedInvoicePdf(config.workspaceRoot, slug, id);
  return new Response(Bun.file(file.path), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `inline; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

export function handleCompanyInvoices(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyInvoices(config.workspaceRoot, slug, year);
  return okResponse({ invoices: data });
}
