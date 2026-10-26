export type DevToolsTarget = {
  type?: string;
  url?: string;
  webSocketDebuggerUrl?: string;
};

/** Select only a browser page target; extension targets are not evidence pages. */
export function selectPageDevToolsTarget(targets: DevToolsTarget[]): string {
  const pages = targets.filter(
    (target) =>
      target.type === "page" && typeof target.webSocketDebuggerUrl === "string",
  );
  const target =
    pages.find((candidate) => candidate.url === "about:blank") ?? pages[0];
  if (!target) throw new Error("Chrome did not expose a DevTools page target");
  return target.webSocketDebuggerUrl!;
}
