import { protocolHeaders } from "./api/_protocol";
/** Native-only password-bound ceremony; one same-origin fetch, no mutation retries. */
export class NativeEnrollmentFailure extends Error {
  constructor(public readonly unconfirmed: boolean) { super("Enrollment request refused"); }
}
async function post(path: "begin" | "confirm", body: object): Promise<unknown> {
  let response: Response;
  let value: unknown;
  try {
    const admission = await protocolHeaders(`/api/auth/mfa-enrollment/${path}`, "POST");
    response = await fetch(`/api/auth/mfa-enrollment/${path}`, {
      method: "POST", credentials: "same-origin", cache: "no-store", redirect: "error",
      headers: { ...admission, "content-type": "application/json" }, body: JSON.stringify(body),
    });
    value = await response.json();
  } catch { throw new NativeEnrollmentFailure(true); }
  if (!response.ok) throw new NativeEnrollmentFailure(![400, 401, 403, 429].includes(response.status));
  if (response.status !== 200) throw new NativeEnrollmentFailure(true);
  return value;
}
export async function beginNativeEnrollment(email: string, password: string): Promise<string> {
  const value = await post("begin", { email, password });
  if (!value || typeof value !== "object" || !("method" in value) || value.method !== "totp" || !("totpURI" in value) || typeof value.totpURI !== "string" || value.totpURI.length > 8192 || !value.totpURI.startsWith("otpauth://totp/") || "backupCodes" in value) throw new NativeEnrollmentFailure(true);
  let parsed: URL;
  try { parsed = new URL(value.totpURI); } catch { throw new NativeEnrollmentFailure(true); }
  const secret = parsed.searchParams.get("secret");
  if (parsed.protocol !== "otpauth:" || parsed.hostname !== "totp" || parsed.username || parsed.password || parsed.hash || !secret || !/^[A-Z2-7]{16,128}$/.test(secret) || parsed.searchParams.getAll("secret").length !== 1 || parsed.searchParams.get("digits") !== "6" || parsed.searchParams.get("period") !== "30") throw new NativeEnrollmentFailure(true);
  return secret;
}
export async function confirmNativeEnrollment(code: string): Promise<string[]> {
  const value = await post("confirm", { code });
  if (!value || typeof value !== "object" || !("status" in value) || value.status !== true || !("backupCodes" in value) || !Array.isArray(value.backupCodes) || value.backupCodes.length !== 10 || new Set(value.backupCodes).size !== 10 || !value.backupCodes.every((c): c is string => typeof c === "string" && /^[A-Za-z0-9]{5}-[A-Za-z0-9]{5}$/.test(c))) throw new NativeEnrollmentFailure(true);
  return value.backupCodes;
}
