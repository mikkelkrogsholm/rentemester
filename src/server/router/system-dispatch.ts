import { ApiError } from "../errors";

/** System routes stay outside the central auth/catalog/error seam. */
export function dispatchSystemRoute(path: string, method: string, handlers: {
  health: () => Response;
  readiness: () => Response;
  cvrStatus: () => Response;
}): Response | null {
  if (path === "/api" || path === "/api/health") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.health();
  }
  if (path === "/api/ready") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.readiness();
  }
  if (path === "/api/system/cvr-status") {
    if (method !== "GET") throw ApiError.methodNotAllowed("kun GET er understøttet på denne rute");
    return handlers.cvrStatus();
  }
  return null;
}
