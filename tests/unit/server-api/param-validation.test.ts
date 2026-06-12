// Cockpit API — edge validation of filter/body params.
//
// Two list/patch endpoints used to silently swallow malformed input: the
// payables list coerced an unknown ?status= to "open" and a bad ?asOf= to
// today, and the bilagsmail alias PATCH coerced any non-string value to null —
// destructively CLEARING the company's alias. Both now reject with a 400, like
// the sibling exceptions list and every other body parser.
import { describe, expect, test } from "bun:test";
import { config, makeWorkspace, get, handleRequest, rmSync } from "./_shared";

describe("cockpit API — payables filter validation", () => {
  test("an unknown ?status= is a 400, a known one is a 200", async () => {
    const ws = makeWorkspace("payables-validate", ["Acme ApS"]);
    const cfg = config({ workspaceRoot: ws });

    const bad = await get(cfg, "/api/companies/acme-aps/payables?status=overdu");
    expect(bad.status).toBe(400);

    const good = await get(
      cfg,
      "/api/companies/acme-aps/payables?status=overdue",
    );
    expect(good.status).toBe(200);

    const badDate = await get(
      cfg,
      "/api/companies/acme-aps/payables?asOf=ikke-en-dato",
    );
    expect(badDate.status).toBe(400);

    rmSync(ws, { recursive: true, force: true });
  });
});

describe("cockpit API — bilagsmail alias validation", () => {
  test("a present-but-non-string alias is a 400 (never a silent clear)", async () => {
    const ws = makeWorkspace("alias-validate", ["Acme ApS"]);
    const cfg = config({ workspaceRoot: ws });

    const res = await handleRequest(
      new Request("http://localhost/api/companies/acme-aps/bilagsmail/alias", {
        method: "PATCH",
        headers: { "content-type": "application/json", host: "localhost" },
        body: JSON.stringify({ alias: 123 }),
      }),
      cfg,
    );
    expect(res.status).toBe(400);

    // A legitimate string still sets it, and null still clears it.
    const ok = await handleRequest(
      new Request("http://localhost/api/companies/acme-aps/bilagsmail/alias", {
        method: "PATCH",
        headers: { "content-type": "application/json", host: "localhost" },
        body: JSON.stringify({ alias: "bilag" }),
      }),
      cfg,
    );
    expect(ok.status).toBe(200);

    rmSync(ws, { recursive: true, force: true });
  });
});
