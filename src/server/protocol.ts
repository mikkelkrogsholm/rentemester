import { createHash } from "node:crypto";

export type ProtocolOperation = { method: string; path: string; capability: string };
export type ProtocolManifest = {
  format: "rentemester-http-protocol/1";
  protocolMajor: 1;
  manifestId: string;
  wireProfiles: string[];
  operations: ProtocolOperation[];
};

// Same identity serialization as the native generator: recursively sorted keys,
// compact JSON, sorted (path, method) operations, manifestId omitted. All current
// catalog strings are ASCII. Never include permissions, identity or company facts.
function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([k, v]) => `${JSON.stringify(k)}:${canonical(v)}`).join(",")}}`;
  return JSON.stringify(value);
}

export function createProtocolManifest(routes: readonly { method: string; pattern: string }[]): ProtocolManifest {
  const seen = new Set<string>();
  const operations = routes.flatMap(({ method, pattern }) => {
    // This catalog's only action parameter dispatches a closed set of literals.
    const patterns = pattern === "/api/companies/:slug/group-dispositions/:action"
      ? ["plan", "propose", "approve", "link", "settle", "supersede", "reopen"].map(action => pattern.replace(":action", action))
      : [pattern];
    return patterns.map(pattern => {
      const path = pattern.replace(/:([A-Za-z][A-Za-z0-9]*)/g, "{$1}");
      if (!/^(GET|HEAD|POST|PUT|PATCH|DELETE)$/.test(method) || !path.startsWith("/api") || /[?*:]/.test(path)) throw new Error("Invalid protocol route");
      const capability = `${method} ${path}`;
      if (seen.has(capability)) throw new Error("Duplicate protocol operation");
      seen.add(capability);
      return { method, path, capability };
    });
  }).sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : a.method < b.method ? -1 : a.method > b.method ? 1 : 0);
  const identity = { format: "rentemester-http-protocol/1" as const, protocolMajor: 1 as const, wireProfiles: ["legacy-react-v1"], operations };
  const manifest = { ...identity, manifestId: createHash("sha256").update(canonical(identity)).digest("hex") };
  if (new TextEncoder().encode(JSON.stringify(manifest)).length > 1024 * 1024) throw new Error("Protocol manifest too large");
  return manifest;
}

export function protocolPathMatches(template: string, path: string): boolean {
  const expected = template.split("/");
  const actual = path.split("/");
  return !path.includes("?") && expected.length === actual.length && expected.every((part, i) => /^\{[^{}]+\}$/.test(part) ? actual[i]!.length > 0 : actual[i] === part);
}

/** None of the headers is explicit legacy-v1 compatibility for all of major 1.
 * Admission is not authentication; the original security pipeline still runs.
 * Refusal never dispatches or retries a business operation.
 */
export function protocolAdmission(request: Request, manifest: ProtocolManifest): Response | null {
  const keys = ["x-rentemester-protocol", "x-rentemester-manifest", "x-rentemester-wire-profile"];
  if (keys.every(key => !request.headers.has(key))) return null;
  if (request.headers.get(keys[0]!) === "1" && request.headers.get(keys[1]!) === manifest.manifestId
      && manifest.wireProfiles.includes(request.headers.get(keys[2]!) ?? "")
      && manifest.operations.some(op => op.method === request.method && protocolPathMatches(op.path, new URL(request.url).pathname))) return null;
  return Response.json({ ok: false, code: "protocol_incompatible", errors: ["HTTP protocol negotiation is incompatible"] }, { status: 409 });
}
