// Shared primitives for the cockpit API client.
//
// `ApiError` is the canonical error class every per-domain module throws via
// `request<T>(...)`. Re-exported from `app/src/lib/api.ts` so external callers
// keep importing it from the barrel.

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

export async function request<T>(path: string, init?: RequestInit): Promise<T> {
  let res: Response;
  try {
    res = await fetch(path, {
      ...init,
      headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
    });
  } catch {
    throw new ApiError(
      "network",
      "Kunne ikke nå serveren. Kører `rentemester serve`?",
      0,
    );
  }

  let body: unknown;
  try {
    body = await res.json();
  } catch {
    throw new ApiError("internal", "Serveren gav et ugyldigt svar.", res.status);
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
    throw new ApiError(code, message, res.status);
  }
  if (!res.ok) {
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
