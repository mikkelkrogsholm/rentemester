/** Browser presentation fence only. Server admission remains authoritative. */
export type WorkspaceMode = "active" | "frozen" | "moved" | "staged";
let current: { scope: string; mode: WorkspaceMode | "unknown" } | null = null;
export function setWorkspaceWriteMode(scope: string, mode: WorkspaceMode | "unknown") { current = { scope, mode }; }
export function clearWorkspaceWriteMode(scope: string) { if (current?.scope === scope) current = null; }
export function ordinaryWorkspaceWriteAllowed(path: string, method: string, readOnly = false) {
  if (!current || current.mode === "active" || readOnly || ["GET", "HEAD", "OPTIONS"].includes(method.toUpperCase())) return true;
  return path.startsWith("/api/auth/") || path === "/api/workspace/backup"
    || /^\/api\/workspace\/transfer\/(freeze|cancel|export|release|activate)$/.test(path)
    || /^\/api\/companies\/[^/]+\/restore\/receipts\/reconcile$/.test(path);
}
