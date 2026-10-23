// Synthetic static route metadata for seam tests, not a production router catalog.
export const seamOperations = [
  ["GET", "/api/read"], ["POST", "/api/write"], ["POST", "/api/plan"],
  ["GET", "/api/companies/{company}/journal"], ["GET", "/api/companies/{company}/journal/export"],
  ["GET", "/api/account"], ["GET", "/api/blob"],
].map(([method, path]) => ({ method, path, capability: `${method} ${path}` }));
export function protocolResponse(overrides: Record<string, unknown> = {}): Response {
  return Response.json({ format: "rentemester-http-protocol/1", protocolMajor: 1,
    manifestId: "a".repeat(64), wireProfiles: ["legacy-react-v1"], operations: seamOperations, ...overrides });
}
export function withProtocol(handler: (input: RequestInfo | URL, init?: RequestInit) => Promise<unknown>) {
  return async (input: RequestInfo | URL, init?: RequestInit): Promise<any> =>
    String(input) === "/api/protocol" ? protocolResponse() : handler(input, init);
}
