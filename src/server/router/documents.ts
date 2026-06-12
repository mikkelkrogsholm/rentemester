// Document list, file serve, and booking-options read handlers.

import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import {
  buildCompanyDocuments,
  buildDocumentBookingOptions,
  resolveCompanyDocumentFile,
} from "../data";
import { okResponse } from "./_shared";

export function handleCompanyDocuments(config: ServerConfig, slug: string): Response {
  const data = buildCompanyDocuments(config.workspaceRoot, slug);
  return okResponse({ documents: data });
}

/**
 * GET /api/companies/:slug/documents/:id/booking-options — the read-side data
 * the Bogfør-bilag modal needs (#407): the document fields to prefill, the
 * bookable expense accounts, and the unmatched outgoing bank transactions the
 * owner can pair the bilag with. A read route, so it bypasses the mutation
 * pipeline; an unknown company / ledger / document is a 404.
 */
export function handleCompanyDocumentBookingOptions(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const data = buildDocumentBookingOptions(config.workspaceRoot, slug, id);
  return okResponse({ options: data });
}

/**
 * GET /api/companies/:slug/documents/:id/file — serves the stored bilag file
 * so a human can open it in the cockpit. A read route, so it does not run the
 * mutation pipeline; an unknown company or document is a 404.
 */
export function handleCompanyDocumentFile(
  config: ServerConfig,
  slug: string,
  idRaw: string,
): Response {
  const id = Number(idRaw);
  if (!Number.isInteger(id) || id <= 0) {
    throw ApiError.badRequest("document id must be a positive integer");
  }
  const file = resolveCompanyDocumentFile(config.workspaceRoot, slug, id);
  // PDFs and images render safely inline; anything else (txt/json/unknown) is
  // sent as a download so the browser never renders it inside the cockpit's
  // own origin. `nosniff` stops the browser re-sniffing the body as HTML.
  // `filename*` carries the (possibly non-ASCII) name per RFC 5987.
  const inline =
    file.mimeType === "application/pdf" || file.mimeType.startsWith("image/");
  return new Response(Bun.file(file.path), {
    headers: {
      "content-type": file.mimeType,
      "content-disposition": `${inline ? "inline" : "attachment"}; filename*=UTF-8''${encodeURIComponent(file.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}
