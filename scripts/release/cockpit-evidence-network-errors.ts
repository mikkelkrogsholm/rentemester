export type OwnedEvidenceRequest = {
  urlPattern: string;
  status: 200 | 403 | 500;
};

export type ExpectedNetworkError = {
  url: string;
  status: 403 | 500;
  source: "network";
  level: "error";
  message: string;
};

type ChromeLogEntry = {
  source?: unknown;
  level?: unknown;
  text?: unknown;
  url?: unknown;
};

/**
 * Chrome reports failed HTTP requests through Log.entryAdded. Only an exact,
 * scenario-owned 403/500 response is expected evidence; every other console
 * event remains a failure for the evidence run.
 */
export function expectedNetworkError(
  params: unknown,
  base: string,
  ownedRequests: OwnedEvidenceRequest[],
): ExpectedNetworkError | undefined {
  if (!params || typeof params !== "object") return;
  const entry = (params as { entry?: ChromeLogEntry }).entry;
  if (!entry || entry.source !== "network" || entry.level !== "error") return;
  if (typeof entry.url !== "string" || typeof entry.text !== "string") return;
  const request = ownedRequests.find(
    (candidate) => entry.url === `${base}${candidate.urlPattern}`,
  );
  if (!request || (request.status !== 403 && request.status !== 500)) return;
  const status = Number(
    /^Failed to load resource: the server responded with a status of (\d{3})(?: \([^)]*\))?$/.exec(entry.text)?.[1],
  );
  if (status !== request.status) return;
  return { url: entry.url, status: request.status, source: "network", level: "error", message: entry.text };
}
