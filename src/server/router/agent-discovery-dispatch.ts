import { ApiError } from "../errors";

/** Agent-discovery dispatch keeps catalog/auth ownership in router.ts. */
export function dispatchAgentDiscoveryRoute(path: string, method: string, handlers: {
  rules: () => Response;
  capabilities: () => Response;
  workflow: (id: string) => Response;
}): Response | null {
  if (path === "/api/rules") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.rules();
  }
  if (path === "/api/agent-capabilities") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.capabilities();
  }
  const workflow = /^\/api\/agent-workflows\/([^/]+)$/.exec(path);
  if (!workflow) return null;
  if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
  return handlers.workflow(decodeURIComponent(workflow[1]!));
}
