import { resolve, sep } from "node:path";

// Deliberately no backend proxy: browser tests intercept API calls with
// synthetic fixtures. Missing interception fails closed on this server.
const root = resolve(import.meta.dir, "../dist");
const port = Number(process.env.RENTEMESTER_BROWSER_TEST_PORT ?? "5379");
if (!Number.isInteger(port) || port < 1024 || port > 65535) throw new Error("Invalid browser test port");
if (!(await Bun.file(resolve(root, "index.html")).exists())) {
  throw new Error("Build the cockpit before running browser verification");
}

const server = Bun.serve({
  hostname: "127.0.0.1",
  port,
  async fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === "/__browser_test_ready") return new Response("synthetic-browser-test-server");
    if (url.pathname.startsWith("/api/")) {
      return Response.json({ ok: false, code: "fixture_required", errors: ["Browser fixture missing"] }, { status: 503 });
    }
    let pathname: string;
    try { pathname = decodeURIComponent(url.pathname); } catch { return new Response("Invalid path", { status: 400 }); }
    const candidate = resolve(root, `.${pathname}`);
    if (candidate !== root && !candidate.startsWith(root + sep)) return new Response("Forbidden", { status: 403 });
    const file = Bun.file(candidate);
    if (await file.exists()) return new Response(file);
    // Asset misses must not silently turn into a successful HTML response.
    if (/\.[a-z0-9]+$/i.test(pathname)) return new Response("Asset not found", { status: 404 });
    return new Response(Bun.file(resolve(root, "index.html")));
  },
});
console.info(`Synthetic browser test server listening at ${server.url}`);
for (const signal of ["SIGTERM", "SIGINT"] as const) process.on(signal, () => { server.stop(true); process.exit(0); });
