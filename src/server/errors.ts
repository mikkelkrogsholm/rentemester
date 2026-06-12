// Safe error mapping for the cockpit backend (#170, cf. #158).
//
// Internal errors carry filesystem paths, SQL fragments and other detail that
// must never reach an HTTP client. Handlers throw `ApiError` for *expected*
// failures (bad input, not found, ...); anything else is caught at the edge
// and mapped to a generic 500 with NO leaked detail.

export type ApiErrorCode =
  | "bad_request"
  | "not_found"
  | "conflict"
  | "unauthorized"
  | "method_not_allowed"
  | "internal";

const STATUS_BY_CODE: Record<ApiErrorCode, number> = {
  bad_request: 400,
  unauthorized: 401,
  not_found: 404,
  method_not_allowed: 405,
  conflict: 409,
  internal: 500,
};

/**
 * An error that is safe to surface to an HTTP client. The `message` is
 * deliberately curated by the thrower — keep it free of paths/secrets.
 *
 * `subcode` (optional) carries the SAME stable, machine-readable marker the
 * MCP envelope's `code` field uses for cross-cutting preconditions —
 * `"CONFIRM_REQUIRED"`, `"CONFIRMTEXT_MISMATCH"`, `"BACKUP_LOCKED"` — so an
 * agent driving both HTTP and MCP can share one error parser. The top-level
 * `code` keeps its 6-value HTTP enum unchanged (Batch F-1).
 */
export class ApiError extends Error {
  readonly code: ApiErrorCode;
  readonly status: number;
  readonly subcode: string | null;

  constructor(code: ApiErrorCode, message: string, options?: { subcode?: string }) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = STATUS_BY_CODE[code];
    this.subcode = options?.subcode ?? null;
  }

  static badRequest(message: string, options?: { subcode?: string }): ApiError {
    return new ApiError("bad_request", message, options);
  }
  static notFound(message: string): ApiError {
    return new ApiError("not_found", message);
  }
  static conflict(message: string, options?: { subcode?: string }): ApiError {
    return new ApiError("conflict", message, options);
  }
  static unauthorized(message: string): ApiError {
    return new ApiError("unauthorized", message);
  }
  static methodNotAllowed(message: string): ApiError {
    return new ApiError("method_not_allowed", message);
  }
}

/**
 * The cockpit HTTP error-envelope shape (#368).
 *
 * Identical in shape and field names to the MCP/CLI envelope
 * (`src/mcp/envelope.ts`), so an agent that drives all three write-stacks can
 * share one error-parser:
 *
 *   { ok: false, errors: ["..."], code?: "bad_request" | ... }
 *
 * The discrete `code` enum survives at the TOP level — it is still useful for
 * programmatic branching (`conflict` vs `bad_request`) — but the
 * human-readable message moved from `error.message` into `errors[0]`. The old
 * singular `error: { code, message }` object is GONE.
 */
export type ApiErrorBody = {
  ok: false;
  errors: string[];
  code: ApiErrorCode;
  /**
   * Optional cross-surface stable marker for cross-cutting preconditions —
   * mirrors the MCP envelope's `code` field (`"CONFIRM_REQUIRED"`,
   * `"CONFIRMTEXT_MISMATCH"`, `"BACKUP_LOCKED"`, ...). Set only when the
   * thrower passes `subcode` to ApiError. An agent that pins on `subcode`
   * gets the same identifier on HTTP and MCP. (Batch F-1)
   */
  subcode?: string;
};

/**
 * Maps any thrown value to a safe `{ status, body }` pair.
 *
 * `ApiError` instances pass their curated message through (as `errors[0]`).
 * Every other error is collapsed to a generic 500 — its real message (which
 * may contain a filesystem path or SQL fragment) is dropped, never serialised.
 */
export function toErrorResponse(err: unknown): {
  status: number;
  body: ApiErrorBody;
} {
  if (err instanceof ApiError) {
    const body: ApiErrorBody = { ok: false, errors: [err.message], code: err.code };
    if (err.subcode) body.subcode = err.subcode;
    return { status: err.status, body };
  }
  return {
    status: 500,
    body: { ok: false, errors: ["intern serverfejl"], code: "internal" },
  };
}
