import type { Page } from "@playwright/test";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import type { CompanyMileage, MileageEntryRow } from "../../app/src/lib/types/mileage";
const saved = "/tmp/rentemester-rust-reports/native-mileage.json";
type Evidence = { year: string; entries: MileageEntryRow[]; totalKm: number; totalAmount: number; tripCount: number };
export async function checkNativeMileage(page: Page, base: string, restarted = false) {
  const url = `${base}/api/companies/synthetic/mileage`;
  const headers = { origin: base, "content-type": "application/json" };
  async function read(year?: string): Promise<CompanyMileage> {
    const response = await page.request.get(`${url}${year ? `?year=${encodeURIComponent(year)}` : ""}`);
    if (!response.ok()) throw new Error(`Mileage read failed: ${await response.text()}`);
    return (await response.json()).mileage;
  }
  let evidence: Evidence;
  if (!restarted) {
    const before = await read();
    const journalBefore = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
    const date = before.periodStart; // Inside selected fiscal year, independent of the host's timezone.
    const input = { tripDate: date, purpose: "Synthetic mileage precision visit", fromLocation: "Synthetic origin", toLocation: "Synthetic destination", kilometers: 12.35, vehicle: "Synthetic private car", driver: "Synthetic driver", ratePerKm: 3.79, rateBasis: "Synthetic user-confirmed rate basis", rateSource: "https://example.invalid/synthetic-rate", notes: "Synthetic mileage audit evidence", confirm: true };
    for (const data of [{ ...input, kilometers: 0 }, { ...input, ratePerKm: -1 }, { ...input, tripDate: "2026-02-30" }, { ...input, rateBasis: " " }, { ...input, confirm: false }]) {
      const denied = await page.request.post(url, { headers, data });
      if (denied.status() !== 422) throw new Error(`Invalid mileage accepted: ${denied.status()}`);
    }
    if ((await page.request.get(`${url}?year=missing`)).status() !== 422) throw new Error("Unknown mileage fiscal year accepted");
    if ((await page.request.get(`${url}?year=2026&year=2025`)).status() !== 400) throw new Error("Duplicate mileage year accepted");
    await page.goto(`${base}/companies/synthetic/koersel`);
    const created: number[] = [];
    for (const [purpose, km, rate] of [[input.purpose, "12.35", "3.79"], ["Synthetic mileage advisory visit", "0.15", "9"]]) {
      await page.getByRole("button", { name: "Registrér kørsel", exact: true }).click();
      const dialog = page.getByRole("dialog", { name: "Registrér kørsel", exact: true });
      for (const [label, value] of Object.entries({ "Dato": date, "Formål": purpose, "Fra-adresse": input.fromLocation, "Til-adresse": input.toLocation, "Antal km": km, "Køretøj": input.vehicle, "Chauffør": input.driver, "Takst (kr/km)": rate, "Takst-kilde (valgfri)": input.rateSource, "Noter (valgfri)": input.notes })) {
        await dialog.getByLabel(label, { exact: true }).fill(value);
      }
      await dialog.getByLabel(/^Takst-grundlag/).fill(input.rateBasis);
      const [response] = await Promise.all([page.waitForResponse(r => r.url().endsWith("/mileage") && r.request().method() === "POST"), dialog.getByRole("button", { name: "Registrér kørsel", exact: true }).click()]);
      if (!response.ok()) throw new Error(`Mileage registration failed: ${await response.text()}`);
      const result = (await response.json()).mileage;
      created.push(result.mileageEntryId);
      await dialog.getByText(/Kørsel registreret som MIL-/).waitFor();
      if (rate === "9") await dialog.getByText(/overstiger Skatterådets højeste maksimum for 2026/).waitFor();
      await dialog.getByRole("button", { name: "Færdig", exact: true }).click();
      await page.getByRole("cell", { name: purpose, exact: true }).waitFor();
    }
    const after = await read(before.selectedYear);
    const entries = after.entries.filter(e => created.includes(e.id));
    if (entries.length !== 2 || entries.find(e => e.id === created[0])?.amountBasis !== 46.81 || entries.find(e => e.id === created[1])?.amountBasis !== 1.35) throw new Error("Mileage arithmetic or persisted entries wrong");
    if (after.tripCount !== before.tripCount + 2 || Math.round((after.totalKilometers - before.totalKilometers) * 100) !== 1250 || Math.round((after.totalAmountBasis - before.totalAmountBasis) * 100) !== 4816) throw new Error("Mileage fiscal totals wrong");
    const journalAfter = await (await page.request.get(`${base}/api/companies/synthetic/journal`)).json();
    if (JSON.stringify(journalBefore.journal.entries) !== JSON.stringify(journalAfter.journal.entries)) throw new Error("Mileage register unexpectedly wrote to journal");
    evidence = { year: before.selectedYear, entries, totalKm: after.totalKilometers, totalAmount: after.totalAmountBasis, tripCount: after.tripCount };
    mkdirSync("/tmp/rentemester-rust-reports", { recursive: true });
    writeFileSync(saved, JSON.stringify(evidence));
  } else evidence = JSON.parse(readFileSync(saved, "utf8"));
  const mileage = await read(evidence.year);
  for (const original of evidence.entries) {
    const row = mileage.entries.find(e => e.id === original.id);
    if (JSON.stringify(row) !== JSON.stringify(original)) throw new Error("Mileage original changed after restart (including number, basis and createdAt)");
  }
  if (mileage.totalKilometers !== evidence.totalKm || mileage.totalAmountBasis !== evidence.totalAmount || mileage.tripCount !== evidence.tripCount) throw new Error("Mileage totals changed after restart");
  const month = mileage.months.find(m => m.month === Number(evidence.entries[0].tripDate.slice(5, 7)));
  const expected = mileage.entries.filter(e => Number(e.tripDate.slice(5, 7)) === month?.month);
  if (!month || month.tripCount !== expected.length || Math.round(month.kilometers * 100) !== expected.reduce((n, e) => n + Math.round(e.kilometers * 100), 0) || Math.round(month.amountBasis * 100) !== expected.reduce((n, e) => n + Math.round(e.amountBasis * 100), 0)) throw new Error("Mileage month totals wrong");
  await page.goto(`${base}/companies/synthetic/koersel?year=${encodeURIComponent(evidence.year)}`);
  await page.getByRole("cell", { name: "Synthetic mileage precision visit", exact: true }).waitFor();
  await page.getByText("Sum pr. måned", { exact: true }).waitFor();
  await page.screenshot({ path: "/tmp/rentemester-rust-mileage.png", fullPage: true });
  console.log(restarted ? "PASS: original mileage entries, numbering, rate basis, timestamps and fiscal/month totals persist after restart" : "PASS: real Kørsel UI → two exact mileage entries → fiscal/month totals and advisory; invalid inputs refused and journal unchanged");
}
