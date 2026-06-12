// Shared helpers for the write-handler slices.
//
// Each handler file in this directory parses route params + body, runs the
// shared `withCompanyMutation` pipeline, and delegates to `src/core/`. The
// small body-parsing helpers + response shape live here so every slice uses
// the same validation surface.

import { ApiError } from "../errors";
import type { InvoiceLineInput } from "../../core/invoice";

/**
 * Max request-body size for the file-upload routes (#213, slices 2-3). A bank
 * CSV or a base64-encoded document is far larger than slice 1's tiny JSON
 * body, but still bounded — 12 MiB comfortably covers a multi-year CSV export
 * or a scanned multi-page PDF (base64 inflates bytes by ~33%) while refusing a
 * body that would exhaust memory. The guard runs in `withCompanyMutation`
 * before the body is read.
 */
export const MAX_UPLOAD_BODY_BYTES = 12 * 1024 * 1024;

export const JSON_HEADERS = { "content-type": "application/json; charset=utf-8" };

export function okResponse(body: Record<string, unknown>, status = 200): Response {
  return new Response(JSON.stringify({ ok: true, ...body }), {
    status,
    headers: JSON_HEADERS,
  });
}

/** Parses a positive-integer path segment, mapping a bad value to a 400. */
export function parseIdParam(raw: string, label: string): number {
  const value = Number(raw);
  if (!Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${label}' must be a positive integer`);
  }
  return value;
}

/** Reads an optional string body field, trimming and collapsing empty to undefined. */
export function optionalBodyString(
  body: Record<string, unknown>,
  key: string,
): string | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "string") {
    throw ApiError.badRequest(`'${key}' must be a string when present`);
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

/** Reads a required, non-empty string body field, mapping a bad value to a 400. */
export function requireBodyString(
  body: Record<string, unknown>,
  key: string,
): string {
  const value = body[key];
  if (typeof value !== "string" || value.trim().length === 0) {
    throw ApiError.badRequest(`'${key}' is required and must be a non-empty string`);
  }
  return value;
}

/** Reads an optional positive-integer body field, mapping a bad value to a 400. */
export function optionalBodyPositiveInt(
  body: Record<string, unknown>,
  key: string,
): number | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${key}' must be a positive integer when present`);
  }
  return value;
}

/**
 * Reads a required positive-integer body field, mapping a bad value to a 400.
 * Used for the invoice document id on the post/settle routes.
 */
export function requireBodyPositiveInt(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isInteger(value) || value <= 0) {
    throw ApiError.badRequest(`'${key}' is required and must be a positive integer`);
  }
  return value;
}

/** Reads an optional boolean body field, mapping a bad value to a 400 (#434). */
export function optionalBodyBoolean(
  body: Record<string, unknown>,
  key: string,
): boolean | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "boolean") {
    throw ApiError.badRequest(`'${key}' must be a boolean when present`);
  }
  return value;
}

/** Reads an optional finite-number body field, mapping a bad value to a 400. */
export function optionalBodyNumber(
  body: Record<string, unknown>,
  key: string,
): number | undefined {
  const value = body[key];
  if (value === undefined || value === null) return undefined;
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw ApiError.badRequest(`'${key}' must be a number when present`);
  }
  return value;
}

/** Reads a required, finite-number body field, mapping a bad value to a 400. */
export function requireBodyNumber(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw ApiError.badRequest(
      `'${key}' is required and must be a finite number`,
    );
  }
  return value;
}

/** Reads a required positive-number body field, mapping a bad value to a 400. */
export function requireBodyPositiveNumber(
  body: Record<string, unknown>,
  key: string,
): number {
  const value = body[key];
  if (typeof value !== "number" || !Number.isFinite(value) || value <= 0) {
    throw ApiError.badRequest(
      `'${key}' is required and must be a positive number`,
    );
  }
  return value;
}

/**
 * Parses the `lines` body field into core `InvoiceLineInput[]`. Each line is
 * the three essentials a human supplies — description, quantity, unit price
 * ex-VAT. Anything malformed is a 400; `computeInvoiceAmounts` performs the
 * deeper numeric validation (quantity > 0, etc.) and is the single source of
 * truth for every derived amount.
 */
export function parseInvoiceLines(raw: unknown): InvoiceLineInput[] {
  if (!Array.isArray(raw) || raw.length === 0) {
    throw ApiError.badRequest("'lines' is required and must be a non-empty array");
  }
  return raw.map((entry, index) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry)) {
      throw ApiError.badRequest(`lines[${index}] must be an object`);
    }
    const line = entry as Record<string, unknown>;
    if (typeof line.description !== "string" || line.description.trim().length === 0) {
      throw ApiError.badRequest(`lines[${index}].description is required and must be a non-empty string`);
    }
    if (typeof line.quantity !== "number" || !Number.isFinite(line.quantity)) {
      throw ApiError.badRequest(`lines[${index}].quantity is required and must be a number`);
    }
    if (typeof line.unitPriceExVat !== "number" || !Number.isFinite(line.unitPriceExVat)) {
      throw ApiError.badRequest(`lines[${index}].unitPriceExVat is required and must be a number`);
    }
    return {
      description: line.description.trim(),
      quantity: line.quantity,
      unitPriceExVat: line.unitPriceExVat,
    };
  });
}

/**
 * Parses an optional invoice party (`buyer` / `seller`) body field into the
 * partial-object shape `InvoicePayload` expects. Each field is optional — the
 * core validator / master-data resolution is the authority on completeness.
 */
export function parseInvoiceParty(
  raw: unknown,
  label: string,
): { name?: string; address?: string; vatOrCvr?: string } {
  if (raw === undefined || raw === null) return {};
  if (typeof raw !== "object" || Array.isArray(raw)) {
    throw ApiError.badRequest(`'${label}' must be an object when present`);
  }
  const p = raw as Record<string, unknown>;
  const party: { name?: string; address?: string; vatOrCvr?: string } = {};
  for (const field of ["name", "address", "vatOrCvr"] as const) {
    const v = p[field];
    if (v === undefined || v === null) continue;
    if (typeof v !== "string") {
      throw ApiError.badRequest(`'${label}.${field}' must be a string when present`);
    }
    const trimmed = v.trim();
    if (trimmed.length > 0) party[field] = trimmed;
  }
  return party;
}
