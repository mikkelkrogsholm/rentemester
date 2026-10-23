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
    if (!absolute && !path.startsWith("/api/")) refuse();
    const url = absolute ? new URL(path) : undefined;
    if (url?.username || url?.password || path.startsWith("//")) refuse();
    const pathname = url ? url.pathname : path.split(/[?#]/)[0]!;
    const segments = pathname.slice(1).split("/");
    if (segments.some(s => !s || /%2f|%5c/i.test(s))) refuse();
    const res = await fetch(url ? `${url.origin}/api/protocol` : "/api/protocol", {
      method: "GET", credentials: "omit", redirect: "error", signal,
    });
    const manifest = validate(await readManifest(res));
    signal?.throwIfAborted();
    if (!manifest.operations.some(op => op.method === method.toUpperCase() && (() => {
      const template = routeSegments(op.path);
      return template.length === segments.length && template.every((s, i) => s.startsWith("{") || s === segments[i]);
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
