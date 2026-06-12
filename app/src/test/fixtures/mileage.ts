import type { CompanyMileage } from "../../lib/types";
import {
  MILEAGE_MONTH_LABELS,
  STATEMENT_COMPANY,
  STATEMENT_FISCAL_YEARS,
} from "./_shared";

export function mileage(
  over: Partial<CompanyMileage> = {},
): CompanyMileage {
  // One trip in March, one in May — exercises the per-month breakdown and
  // the "newest trip first" ordering of the entries table.
  const months = MILEAGE_MONTH_LABELS.map((label, i) => ({
    month: i + 1,
    label,
    tripCount: i === 2 || i === 4 ? 1 : 0,
    kilometers: i === 2 ? 312 : i === 4 ? 84 : 0,
    amountBasis: i === 2 ? 1182.48 : i === 4 ? 318.36 : 0,
  }));
  return {
    slug: "acme-aps",
    selectedYear: "2026",
    archived: false,
    company: STATEMENT_COMPANY,
    fiscalYears: STATEMENT_FISCAL_YEARS,
    periodStart: "2026-01-01",
    periodEnd: "2026-12-31",
    entries: [
      {
        id: 2,
        entryNo: "MIL-2026-000002",
        tripDate: "2026-05-10",
        purpose: "Møde Odense",
        fromLocation: "København",
        toLocation: "Odense",
        kilometers: 84,
        vehicle: "Privat bil",
        driver: "Owner",
        ratePerKm: 3.79,
        amountBasis: 318.36,
        rateBasis: "SKAT 2026, høj sats",
        rateSource: null,
        notes: null,
        createdAt: "2026-05-10T08:00:00.000Z",
      },
      {
        id: 1,
        entryNo: "MIL-2026-000001",
        tripDate: "2026-03-15",
        purpose: "Kundebesøg Aarhus",
        fromLocation: "København",
        toLocation: "Aarhus",
        kilometers: 312,
        vehicle: "Privat bil",
        driver: "Owner",
        ratePerKm: 3.79,
        amountBasis: 1182.48,
        rateBasis: "SKAT 2026, høj sats",
        rateSource: "https://skat.dk/",
        notes: null,
        createdAt: "2026-03-15T08:00:00.000Z",
      },
    ],
    totalKilometers: 396,
    totalAmountBasis: 1500.84,
    tripCount: 2,
    months,
    ...over,
  };
}
