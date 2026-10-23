// Passive route support only: this metadata never grants user permissions.
// Deliberately uncached: independent origins/restarted servers cannot inherit admission.
const MAX_MANIFEST_BYTES = 1024 * 1024;
const PROFILE = "legacy-react-v1";
const METHODS = new Set(["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE"]);
type Operation = { method: string; path: string; capability: string };
type Manifest = { format: string; protocolMajor: number; manifestId: string; wireProfiles: string[]; operations: Operation[] };

export class ProtocolAdmissionError extends Error {
  constructor() {
    super("Serverens protokol kunne ikke bekræftes. Handlingen blev ikke sendt.");
    this.name = "ProtocolAdmissionError";
  }
}

function refuse(): never { throw new ProtocolAdmissionError(); }
function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
function routeSegments(path: string): string[] {
  if (path === "/api") return ["api"];
  if (!path.startsWith("/api/") || /[?#*\\]/.test(path)) refuse();
  const segments = path.slice(1).split("/");
  if (segments.some(segment => !segment || segment === "." || segment === ".." || (/[{}]/.test(segment) && !/^\{[A-Za-z_][A-Za-z0-9_]*\}$/.test(segment)))) refuse();
  return segments;
}
function validate(value: unknown): Manifest {
  if (!record(value) || value.format !== "rentemester-http-protocol/1" || value.protocolMajor !== 1
    || typeof value.manifestId !== "string" || !/^[a-f0-9]{64}$/.test(value.manifestId)
    || !Array.isArray(value.wireProfiles) || !value.wireProfiles.every(p => typeof p === "string")
    || !value.wireProfiles.includes(PROFILE) || new Set(value.wireProfiles).size !== value.wireProfiles.length
    || !Array.isArray(value.operations)) refuse();
  const seen = new Set<string>();
  for (const op of value.operations) {
    if (!record(op) || typeof op.method !== "string" || !METHODS.has(op.method) || typeof op.path !== "string"
      || op.capability !== `${op.method} ${op.path}`) refuse();
    routeSegments(op.path);
    if (seen.has(op.capability as string)) refuse();
    seen.add(op.capability as string);
  }
  return value as Manifest;
}

async function readManifest(res: Response): Promise<unknown> {
  if (!res.ok || !res.body) refuse();
  const declared = res.headers.get("content-length");
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > MAX_MANIFEST_BYTES)) refuse();
  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > MAX_MANIFEST_BYTES) { await reader.cancel(); refuse(); }
      chunks.push(value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}

export async function protocolHeaders(path: string, method: string, signal?: AbortSignal | null): Promise<Record<string, string>> {
  try {
    signal?.throwIfAborted();
    // Relative calls remain relative, preserving the browser's configured origin.
    const absolute = /^https?:\/\//.test(path);
    if (!absolute && path !== "/api" && !path.startsWith("/api/")) refuse();
    const url = absolute ? new URL(path) : undefined;
    if (url?.username || url?.password || path.startsWith("//")) refuse();
    const pathname = url ? url.pathname : path.split(/[?#]/)[0]!;
    const segments = pathname.slice(1).split("/");
    // Company and all ordinary route parameters still reject encoded separators.
    // Inbox source IDs alone are opaque DB values; the matched template below
    // must explicitly identify that source parameter before admission.
    if (segments.some((s, i) => !s || /%5c/i.test(s) || (/%2f/i.test(s)
      && !(i === 4 && segments[0] === "api" && segments[1] === "companies" && segments[3] === "workspace-inbox")))) refuse();
    const res = await fetch(url ? `${url.origin}/api/protocol` : "/api/protocol", {
      method: "GET", credentials: "omit", redirect: "error", signal,
    });
    const manifest = validate(await readManifest(res));
    signal?.throwIfAborted();
    if (!manifest.operations.some(op => op.method === method.toUpperCase() && (() => {
      const template = routeSegments(op.path);
      return template.length === segments.length && template.every((s, i) => {
        const actual = segments[i]!;
        if (!s.startsWith("{")) return s === actual;
        if (!/%2f/i.test(actual)) return true;
        if (i !== 4 || template[0] !== "api" || template[1] !== "companies"
          || template[3] !== "workspace-inbox" || !["{source}", "{sourceId}"].includes(s)
          || ![5, 6].includes(template.length)
          || (template.length === 6 && !["original", "review", "assign", "complete"].includes(template[5]!))) return false;
        try {
          const source = decodeURIComponent(actual);
          return source.length > 0 && new TextEncoder().encode(source).length <= 100
            && !/[\0-\x1f\x7f\\]/.test(source) && !source.split("/").some(p => p === "." || p === "..");
        } catch { return false; }
      });
    })())) refuse();
    return {
      "x-rentemester-protocol": "1",
      "x-rentemester-manifest": manifest.manifestId,
      "x-rentemester-wire-profile": PROFILE,
    };
  } catch (cause) {
    signal?.throwIfAborted();
    if (cause instanceof Error && cause.name === "AbortError") throw cause;
    throw new ProtocolAdmissionError();
  }
}
