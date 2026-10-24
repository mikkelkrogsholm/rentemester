import { createAuthClient } from "better-auth/client";
import { protocolHeaders } from "./api/_protocol";
import { twoFactorClient } from "better-auth/client/plugins";

/** Better Fetch customFetchImpl is supported by installed Better Auth 1.7.1.
 * Admission precedes credential transmission; redirects cannot forward secrets. */
export async function admittedAuthFetch(input: string | URL | Request, init?: RequestInit): Promise<Response> {
  const origin = typeof window === "undefined" ? "http://localhost" : window.location.origin;
  const url = new URL(input instanceof Request ? input.url : String(input), origin);
  if (url.origin !== origin || url.username || url.password || !url.pathname.startsWith("/api/auth/")) throw new Error("Auth origin refused");
  const headers = new Headers(init?.headers);
  const admission = await protocolHeaders(url.pathname, init?.method ?? "GET", init?.signal);
  for (const [key, value] of Object.entries(admission)) headers.set(key, value);
  return fetch(url.toString(), { ...init, headers, credentials: "same-origin", redirect: "error", cache: "no-store" });
}

/** Browser-only Better Auth client. The cockpit and API share one origin. */
export const authClient = createAuthClient({
  baseURL: typeof window === "undefined" ? "http://localhost" : window.location.origin,
  basePath: "/api/auth",
  plugins: [twoFactorClient()],
  fetchOptions: { retry: 0, customFetchImpl: admittedAuthFetch },
});
