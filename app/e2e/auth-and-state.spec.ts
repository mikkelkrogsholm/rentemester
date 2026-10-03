import { expect, test } from "@playwright/test";
import { COMPANY_SLUG, mockApi } from "./fixtures";

for (const profile of ["local", "local-container", "hosted"] as const) {
  test(`${profile} retains the verified deployment gate`, async ({ page }) => {
    const fixture = await mockApi(page, { profile });
    await page.goto("/");
    await expect(page.getByRole("main")).toContainText("Acme ApS");
    fixture.assertComplete();
  });
}

for (const response of [
  { body: { ok: true, deploymentProfile: "unknown" }, status: 200 },
  { body: { ok: false, code: "unavailable", errors: ["Synthetic deployment failure"] }, status: 503 },
]) {
  test(`deployment gate fails closed on ${response.status}/${JSON.stringify(response.body)}`, async ({ page }) => {
    const fixture = await mockApi(page, { overrides: { "GET /api/health": response } });
    await page.goto("/");
    await expect(page.getByRole("alert")).toContainText("sikkerhedsprofil");
    await expect(page.getByText("Acme ApS")).toHaveCount(0);
    expect(fixture.calls.filter((call) => call.path !== "/api/health")).toEqual([]);
    fixture.assertComplete();
  });
}

for (const item of [
  { path: "/", session: "absent", title: "Log ind" },
  { path: "/forgot-password", session: "absent", title: "Nulstil adgangskode" },
  { path: "/reset-password?token=synthetic-secret", session: "absent", title: "Vælg ny adgangskode" },
  { path: "/verify-email", session: "absent", title: "Bekræft din e-mail" },
  { path: "/invite?token=synthetic-invitation", session: "absent", title: "Acceptér invitation" },
  { path: `/companies/${COMPANY_SLUG}/bilag`, session: "unverified", title: "Bekræft din e-mail" },
  { path: `/companies/${COMPANY_SLUG}/bilag`, session: "mfa-required", title: "Opsæt totrinsbekræftelse" },
] as const) {
  test(`hosted ${item.session} shows ${item.title}`, async ({ page }) => {
    const fixture = await mockApi(page, { profile: "hosted", session: item.session });
    await page.goto(item.path);
    await expect(page.getByRole("main")).toContainText(item.title);
    await expect(page.getByText("DOC-2026-000001")).toHaveCount(0);
    expect(fixture.calls.some((call) => call.path.endsWith("/documents"))).toBe(false);
    fixture.assertComplete();
  });
}

test("recovery keeps the same public result for unknown accounts", async ({ page }) => {
  const fixture = await mockApi(page, { profile: "hosted", session: "absent", overrides: {
    "POST /api/auth/request-password-reset": { status: 400, body: { code: "UNKNOWN_ACCOUNT", message: "Unknown synthetic account" } },
  } });
  await page.goto("/forgot-password");
  await page.getByLabel("E-mail", { exact: true }).fill("absent@example.invalid");
  await page.getByRole("button", { name: "Send reset-link" }).click();
  await expect(page.getByRole("status")).toContainText("Hvis e-mailadressen kan bruges");
  await expect(page.getByLabel("E-mail", { exact: true })).toHaveValue("");
  fixture.assertComplete();
});

test("expired hosted session removes already displayed company data", async ({ page }) => {
  const fixture = await mockApi(page, { profile: "hosted", overrides: {
    "GET /api/companies/acme-aps/invoices": { status: 401, body: { ok: false, code: "unauthorized", errors: ["Synthetic session expired"] } },
  } });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  await expect(page.getByRole("main")).toContainText("DOC-2026-000001");
  await page.goto(`/companies/${COMPANY_SLUG}/fakturaer?year=2026`);
  await expect(page.getByRole("main")).toContainText("Log ind");
  await expect(page.getByText("DOC-2026-000001")).toHaveCount(0);
  fixture.assertComplete();
});

for (const role of ["owner", "bookkeeper", "reviewer", "reader"] as const) {
  test(`company ${role} sees only authorized document upload`, async ({ page }) => {
    const fixture = await mockApi(page, { profile: "hosted", role, workspaceRole: "member" });
    await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
    await expect(page.getByRole("main")).toContainText("DOC-2026-000001");
    const upload = page.getByRole("button", { name: "Indlæs bilag", exact: true });
    if (role === "owner" || role === "bookkeeper") await expect(upload).toBeVisible();
    else await expect(upload).toHaveCount(0);
    await expect(page.getByRole("link", { name: "Tilføj virksomhed", exact: true })).toHaveCount(0);
    fixture.assertComplete();
  });
}

test("workspace owner gains no implicit company access", async ({ page }) => {
  const fixture = await mockApi(page, { profile: "hosted", workspaceRole: "workspace_owner", companyAccess: false });
  await page.goto(`/companies/${COMPANY_SLUG}/bilag?year=2026`);
  await expect(page.getByRole("main")).toContainText(/adgang/i);
  await expect(page.getByText("DOC-2026-000001")).toHaveCount(0);
  expect(fixture.calls.some((call) => call.path.endsWith("/documents"))).toBe(false);
  fixture.assertComplete();
});
