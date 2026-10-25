import { expect, test } from "@playwright/test";
import { mockApi } from "./fixtures";
import core from "./data/core.json" with { type: "json" };

// Synthetic browser projection only: not Rust/Tauri/VoiceOver qualification.
const entries = Array.from({ length: 10_000 }, (_, index) => ({
  ...core.journal.entries[0]!, id: index + 1,
  entryNo: `LARGE-${String(index + 1).padStart(5, "0")}`,
  text: `Syntetisk post ${index + 1}`, documentId: 1, total: 1250,
  lines: [
    { ...core.journal.entries[0]!.lines[0]!, journalLineId: index * 2 + 1, debit: 1250, credit: 0 },
    { ...core.journal.entries[0]!.lines[1]!, journalLineId: index * 2 + 2, debit: 0, credit: 1250 },
  ],
}));
const protocol = { body: {
  format: "rentemester-http-protocol/1", protocolMajor: 1,
  manifestId: "a".repeat(64), wireProfiles: ["legacy-react-v1"],
  operations: ["/api/health", "/api/me", "/api/companies", "/api/companies/{slug}/journal", "/api/companies/{slug}/dimensions/{id}"].map(path => ({ method: "GET", path, capability: `GET ${path}` })),
} };
const journal = { ...core.journal, entries };
const url = "/companies/acme-aps/posteringer?year=2026";

test("10,000 entries / 20,000 lines: bounded DOM, dynamic rows, drafts, keyboard and filters", async ({ page }, testInfo) => {
  const fixture = await mockApi(page, { overrides: {
    "GET /api/protocol": protocol,
    "GET /api/companies/acme-aps/journal": { body: { ok: true, journal } },
    ...Object.fromEntries([1, 2, 19999, 20000].map(id => [`GET /api/companies/acme-aps/dimensions/${id}`, { body: { ok: true, assignments: [] } }])),
  } });
  const timings: Record<string, number> = {};
  let start = performance.now();
  await page.goto(url);
  const summaries = page.locator(".entry-summary");
  const viewport = page.locator(".journal-virtual-viewport");
  await expect(page.locator(".statement-asof")).toContainText("10000 posteringer");
  await expect(summaries.first()).toContainText("LARGE-00001");
  timings.renderMs = performance.now() - start;
  expect(await summaries.count()).toBeLessThan(40);
  const first = page.locator('.entry-item[aria-posinset="1"]');
  await expect(first).toHaveAttribute("aria-setsize", "10000");
  const height = (await first.boundingBox())!.height;
  await first.locator(".entry-summary").click();
  expect((await first.boundingBox())!.height).toBeGreaterThan(height + 60);
  await expect(first.locator(".entry-bilag-link")).toHaveAttribute("href", /\/api\/companies\/acme-aps\/documents\/1\/file/);
  await expect(first.locator(".entry-bilag-link")).toHaveAttribute("target", "_blank");
  await first.locator(".dimension-assignment > summary").first().click();
  await first.getByRole("button", { name: "Fordel på dimensioner", exact: true }).first().click();
  const draft = first.getByRole("textbox", { name: "Dimension 1", exact: true });
  await draft.fill("retained-project");
  start = performance.now();
  await viewport.evaluate(element => { element.scrollTop = element.scrollHeight; });
  const last = page.locator('.entry-item[aria-posinset="10000"]');
  await expect(last).toBeAttached();
  timings.scrollEndMs = performance.now() - start;
  await expect(draft).toHaveValue("retained-project");
  await expect(draft).toBeFocused();
  expect(await summaries.count()).toBeLessThan(40);
  await last.locator(".entry-summary").focus();
  await page.keyboard.press("Home");
  await expect(first.locator(".entry-summary")).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(page.locator('.entry-item[aria-posinset="2"] .entry-summary')).toBeFocused();
  await page.keyboard.press("End");
  await expect(last.locator(".entry-summary")).toBeFocused();
  await page.keyboard.press("PageUp");
  await expect(page.locator(".entry-summary:focus")).toHaveCount(1);
  // PageUp legitimately unmounts the last collapsed row. Navigate back to it
  // through the supported keyboard path before expanding, never click a stale locator.
  await page.keyboard.press("End");
  await expect(last.locator(".entry-summary")).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(last.locator("table tbody tr")).toHaveCount(2);
  start = performance.now();
  await viewport.evaluate(element => { element.scrollTop = 0; });
  await expect(first.locator(".entry-summary")).toBeInViewport();
  timings.scrollBackMs = performance.now() - start;
  await expect(draft).toHaveValue("retained-project");
  // Existing URL-navigation guard intentionally blocks filtering a dirty draft.
  // Explicitly discard after verifying scroll retention, then test filtering.
  await first.getByRole("button", { name: "Annullér", exact: true }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Kassér ændringer", exact: true }).click();
  start = performance.now();
  await page.getByRole("searchbox").fill("LARGE-10000");
  await expect(page.locator(".statement-asof")).toContainText("1 af 10000");
  await expect(summaries).toHaveCount(1);
  await expect(summaries).toContainText("LARGE-10000");
  timings.filterMs = performance.now() - start;
  start = performance.now();
  await page.getByRole("button", { name: "Ryd filtre", exact: true }).click();
  await expect(summaries.first()).toContainText("LARGE-00001");
  timings.clearMs = performance.now() - start;
  await page.getByRole("searchbox").fill("LARGE-00002");
  await expect(summaries).toHaveCount(1);
  await expect(summaries).toContainText("LARGE-00002");
  expect(fixture.calls.filter(call => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
  await testInfo.attach("synthetic-browser-timings", { contentType: "application/json", body: JSON.stringify({ entries: 10000, lines: 20000, mountedSummaryLimit: 40, timings }, null, 2) });
});

test("identity changes reset the window; late year and failed foreign reads never leak rows", async ({ page }, testInfo) => {
  let release: () => void = () => {};
  const late = new Promise<void>(resolve => { release = resolve; });
  const fixture = await mockApi(page, { overrides: {
    "GET /api/protocol": protocol,
    "GET /api/companies/acme-aps/journal": async incoming => {
      const params = new URL(incoming.request().url()).searchParams;
      if (params.get("year") === "2025") {
        await late;
        return { body: { ok: true, journal: { ...journal, selectedYear: "2025", entries: [{ ...entries[0]!, text: "LATE FOREIGN YEAR" }] } } };
      }
      if (params.get("source") === "archive") return { status: 503, body: { ok: false, errors: ["Synthetic archive unavailable"] } };
      return { body: { ok: true, journal } };
    },
    "GET /api/companies/beta-aps/journal": { delayMs: 200, body: { ok: true, journal: { ...journal, slug: "beta-aps", company: { ...journal.company, name: "Beta ApS" }, entries: entries.map(entry => ({ ...entry, text: `BETA ${entry.id}` })) } } },
  } });
  const start = performance.now();
  await page.goto(url);
  await expect(page.locator(".entry-summary").first()).toContainText("Syntetisk post 1");
  await page.locator(".entry-summary").first().focus();
  await page.keyboard.press("End");
  await expect(page.locator(".entry-summary:focus")).toContainText("LARGE-10000");
  const navigate = async (next: string) => page.evaluate(path => { history.pushState(null, "", path); window.dispatchEvent(new PopStateEvent("popstate")); }, next);
  const pending = page.waitForRequest(request => request.url().includes("journal?year=2025"));
  await navigate("/companies/acme-aps/posteringer?year=2025");
  await pending;
  await expect(page.locator(".entry-summary")).toHaveCount(0);
  await navigate("/companies/beta-aps/posteringer?year=2026");
  await expect(page.locator(".entry-summary").first()).toContainText("BETA 1");
  release();
  await expect(page.getByText("LATE FOREIGN YEAR", { exact: true })).toHaveCount(0);
  await expect(page.locator(".entry-summary").filter({ hasText: "Syntetisk post" })).toHaveCount(0);
  await expect(page.locator('.entry-item[aria-posinset="1"]')).toBeInViewport();
  await navigate(`${url}&source=archive`);
  await expect(page.locator(".entry-summary")).toHaveCount(0);
  await expect(page.getByText(/Synthetic archive unavailable/)).toBeVisible();
  await navigate(url);
  await expect(page.locator(".entry-summary").first()).toContainText("Syntetisk post 1");
  await expect(page.locator(".entry-item.open")).toHaveCount(0);
  expect(await page.locator(".entry-summary").count()).toBeLessThan(40);
  expect(fixture.calls.filter(call => call.method !== "GET")).toEqual([]);
  fixture.assertComplete();
  await testInfo.attach("synthetic-identity-timing", { contentType: "application/json", body: JSON.stringify({ elapsedMs: performance.now() - start }) });
});
