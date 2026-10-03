// Shared primitives for the cockpit API client.
//
// `ApiError` is the canonical error class every per-domain module throws via
// `request<T>(...)`. Re-exported from `app/src/lib/api.ts` so external callers
// keep importing it from the barrel.

/** Optional cancellation belongs to one read request, never shared state. */
export type ReadRequestOptions = Pick<RequestInit, "signal">;

function isAborted(cause: unknown, signal: AbortSignal | null | undefined): boolean {
  return signal?.aborted === true || (cause instanceof Error && cause.name === "AbortError");
}

/** A failed API call — carries the backend error code for precise handling. */
export class ApiError extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, message: string, status: number) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.status = status;
  }
}

/** Raised once for an expired hosted browser session.  The app shell listens
 * for this event so an old view is never left visible after a 401. */
function signalAuthExpired(): void {
  if (typeof window !== "undefined") {
    window.dispatchEvent(new Event("rentemester:auth-expired"));
  }
}

export async function request<T>(path: string, init?: RequestInit & { readOnly?: boolean }): Promise<T> {
  const { readOnly, ...fetchOptions } = init ?? {};
  const write = !readOnly && init?.method !== undefined && !["GET", "HEAD"].includes(init.method.toUpperCase());
  let res: Response;
  try {
    res = await fetch(path, {
      ...fetchOptions,
      credentials: "same-origin",
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    });
  } catch (cause) {
    if (isAborted(cause, init?.signal)) throw cause;
    throw new ApiError(
      "network",
      write ? "Handlingen kan være gennemført, men serverens resultat kunne ikke bekræftes. Kontrollér status, før du starter en ny handling." : "Kunne ikke nå serveren. Kontrollér forbindelsen og prøv igen.",
      0,
    );
  }

  let body: unknown;
  try {
    body = await res.json();
  } catch (cause) {
    if (isAborted(cause, init?.signal)) throw cause;
    if (res.status === 401) {
      signalAuthExpired();
      throw new ApiError("unauthorized", "Din session er udløbet. Log ind igen.", 401);
    }
    throw new ApiError("internal", write ? "Serverens svar på handlingen kunne ikke læses. Kontrollér status, før du starter en ny handling." : "Serveren gav et ugyldigt svar.", res.status);
  }

  if (body && typeof body === "object" && (body as { ok?: unknown }).ok === false) {
    // #368: cockpit, MCP and CLI all return the same shape now —
    // `{ ok:false, errors:[string], code?:string }`. The human-readable
    // message lives in `errors[0]`; `code` is the discrete enum
    // (`bad_request`, `conflict`, …) for programmatic branching.
    const env = body as { errors?: unknown; code?: unknown };
    const errors = Array.isArray(env.errors)
      ? env.errors.map((e) => String(e))
      : [];
    const code = typeof env.code === "string" ? env.code : "internal";
    const message = errors[0] ?? "Ukendt serverfejl.";
    if (res.status === 401 || code === "unauthorized") signalAuthExpired();
    throw new ApiError(res.status === 401 ? "unauthorized" : code, message, res.status);
  }
  if (!res.ok) {
    if (res.status === 401) {
      signalAuthExpired();
      throw new ApiError("unauthorized", "Din session er udløbet. Log ind igen.", 401);
    }
    throw new ApiError("internal", `HTTP ${res.status}`, res.status);
  }
  return body as T;
}

/** Picks the filename from a `filename*=UTF-8''…` content-disposition header. */
export function parseFilenameFromContentDisposition(cd: string | null): string | null {
  if (!cd) return null;
  const m = cd.match(/filename\*=UTF-8''([^;]+)/i);
  if (!m || !m[1]) return null;
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return null;
  }
}
