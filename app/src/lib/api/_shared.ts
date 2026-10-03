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

type ApiRequestOptions = RequestInit & { readOnly?: boolean };

function isWrite(init?: ApiRequestOptions): boolean {
  return !init?.readOnly && init?.method !== undefined && !["GET", "HEAD"].includes(init.method.toUpperCase());
}

async function fetchResponse(path: string, init?: ApiRequestOptions): Promise<Response> {
  const { readOnly, ...fetchOptions } = init ?? {};
  try {
    return await fetch(path, {
      ...fetchOptions,
      credentials: "same-origin",
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    });
  } catch (cause) {
    if (isAborted(cause, init?.signal)) throw cause;
    throw new ApiError(
      "network",
      isWrite(init) ? "Handlingen kan være gennemført, men serverens resultat kunne ikke bekræftes. Kontrollér status, før du starter en ny handling." : "Kunne ikke nå serveren. Kontrollér forbindelsen og prøv igen.",
      0,
    );
  }
}

/** Decode the same server envelope for JSON responses and binary failures. */
function responseError(res: Response, body: unknown, fallback: string): ApiError {
  const env = body && typeof body === "object" ? body as { errors?: unknown; code?: unknown } : undefined;
  const errors = Array.isArray(env?.errors) ? env.errors.map(String) : [];
  const code = res.status === 401 ? "unauthorized" : typeof env?.code === "string" ? env.code : "internal";
  if (code === "unauthorized") signalAuthExpired();
  return new ApiError(code, errors[0] ?? (res.status === 401 ? "Din session er udløbet. Log ind igen." : fallback), res.status);
}

function bodyReadError(res: Response, cause: unknown, init?: ApiRequestOptions): unknown {
  if (isAborted(cause, init?.signal)) return cause;
  return responseError(res, undefined, isWrite(init)
    ? "Serverens svar på handlingen kunne ikke læses. Kontrollér status, før du starter en ny handling."
    : "Serveren gav et ugyldigt svar.");
}

export async function request<T>(path: string, init?: ApiRequestOptions): Promise<T> {
  const res = await fetchResponse(path, init);
  let body: unknown;
  try {
    body = await res.json();
  } catch (cause) {
    throw bodyReadError(res, cause, init);
  }
  if (body && typeof body === "object" && (body as { ok?: unknown }).ok === false) throw responseError(res, body, "Ukendt serverfejl.");
  if (!res.ok) throw responseError(res, undefined, `HTTP ${res.status}`);
  return body as T;
}

/** Binary success keeps domain headers; transport failures use the canonical seam. */
export async function requestBlob(path: string, init?: ApiRequestOptions): Promise<{ blob: Blob; headers: Headers }> {
  const res = await fetchResponse(path, init);
  if (!res.ok) {
    let body: unknown;
    try { body = await res.json(); } catch (cause) {
      if (isAborted(cause, init?.signal)) throw cause;
    }
    throw responseError(res, body, `HTTP ${res.status}`);
  }
  try {
    return { blob: await res.blob(), headers: res.headers };
  } catch (cause) {
    throw bodyReadError(res, cause, init);
  }
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
