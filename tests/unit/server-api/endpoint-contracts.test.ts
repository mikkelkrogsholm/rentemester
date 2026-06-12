import { describe, expect, test } from "bun:test";
import {
  config,
  get,
  loadWorkspaceManifest,
  makeWorkspace,
  rmSync,
  saveWorkspaceManifest,
} from "./_shared";

describe("cockpit API — endpoint contracts", () => {
  test("GET /api/health reports the service", async () => {
    const ws = makeWorkspace("ep-health");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/health");
      expect(res.status).toBe(200);
      expect(res.body).toMatchObject({ ok: true, service: "rentemester-cockpit" });
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies lists workspace companies", async () => {
    const ws = makeWorkspace("ep-companies", ["Acme ApS", "Beta IVS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies");
      expect(res.status).toBe(200);
      expect(res.body.count).toBe(2);
      const slugs = res.body.companies.map((c: any) => c.slug).sort();
      expect(slugs).toEqual(["acme-aps", "beta-ivs"]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies discovers a populated but unlisted company dir (#256)", async () => {
    // An owner set the company up via the CLI: its directory + ledger sit in
    // the workspace but the cockpit manifest never recorded it. Pre-#256 the
    // cockpit showed "0 virksomheder" and a create-company would mint an empty
    // ledger over it. The cockpit must instead discover and adopt it.
    const ws = makeWorkspace("ep-discover", ["Acme ApS"]);
    try {
      // Drop the company from the manifest, leaving the directory + ledger.
      const manifest = loadWorkspaceManifest(ws);
      saveWorkspaceManifest(ws, { ...manifest, companies: [] });
      // Before discovery the manifest is empty…
      expect(loadWorkspaceManifest(ws).companies).toHaveLength(0);

      const res = await get(config({ workspaceRoot: ws }), "/api/companies");
      expect(res.status).toBe(200);
      // …yet the cockpit surfaces the real company, not "0 virksomheder".
      expect(res.body.count).toBe(1);
      expect(res.body.companies[0].slug).toBe("acme-aps");
      expect(res.body.companies[0].name).toBe("Acme ApS");
      // The discovery is persisted: the manifest now records the company.
      expect(loadWorkspaceManifest(ws).companies.map((c) => c.slug)).toEqual([
        "acme-aps",
      ]);
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/portfolio discovers an unlisted company dir (#256)", async () => {
    // The portfolio is the cockpit's landing page — an owner who set a company
    // up via the CLI must land on it, not on the empty-workspace onboarding.
    const ws = makeWorkspace("ep-discover-pf", ["Acme ApS"]);
    try {
      const manifest = loadWorkspaceManifest(ws);
      saveWorkspaceManifest(ws, { ...manifest, companies: [] });
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio");
      expect(res.status).toBe(200);
      expect(res.body.portfolio.companyCount).toBe(1);
      expect(res.body.portfolio.companies[0].slug).toBe("acme-aps");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET /api/companies/:slug/dashboard returns dashboard data", async () => {
    const ws = makeWorkspace("ep-dashboard", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/acme-aps/dashboard");
      expect(res.status).toBe(200);
      expect(res.body.dashboard.slug).toBe("acme-aps");
      expect(res.body.dashboard.company.name).toBe("Acme ApS");
      expect(res.body.dashboard.invoices.count).toBe(0);
      expect(res.body.dashboard).toHaveProperty("vat");
      expect(res.body.dashboard).toHaveProperty("audit");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("GET dashboard for an unknown slug is a safe 404", async () => {
    const ws = makeWorkspace("ep-dashboard-404", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/ghost/dashboard");
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an unknown endpoint is a safe 404", async () => {
    const ws = makeWorkspace("ep-unknown");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/nope");
      expect(res.status).toBe(404);
      expect(res.body.code).toBe("not_found");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("a wrong method on a known route is 405", async () => {
    const ws = makeWorkspace("ep-405");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio", {
        method: "DELETE",
      });
      expect(res.status).toBe(405);
      expect(res.body.code).toBe("method_not_allowed");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("an invalid asOf query value is rejected with a safe 400", async () => {
    const ws = makeWorkspace("ep-badasof");
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/portfolio?asOf=not-a-date");
      expect(res.status).toBe(400);
      expect(res.body.code).toBe("bad_request");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });

  test("error responses never leak a filesystem path", async () => {
    const ws = makeWorkspace("ep-noleak", ["Acme ApS"]);
    try {
      const res = await get(config({ workspaceRoot: ws }), "/api/companies/ghost/dashboard");
      expect(JSON.stringify(res.body)).not.toContain(ws);
      expect(JSON.stringify(res.body)).not.toContain("/");
    } finally {
      rmSync(ws, { recursive: true, force: true });
    }
  });
});
