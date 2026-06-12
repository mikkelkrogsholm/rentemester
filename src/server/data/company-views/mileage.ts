import {
  buildMileagePeriodReport,
  type MileageEntryRow as CoreMileageEntryRow,
} from "../../../core/mileage";
import {
  resolveStatementContext,
  roundKroner,
  statementCompanyBlock,
  MONTH_NAMES_DK,
} from "../shared";
import type { FiscalYearEntry, StatementCompanyBlock } from "../shared";

// --------------------------------------------------------------------------
// Mileage (Kørsel, #335) — append-only mileage log per fiscal year.
// --------------------------------------------------------------------------

/** A single mileage entry row as the cockpit consumes it. */
export type MileageEntryRow = {
  id: number;
  entryNo: string;
  tripDate: string;
  purpose: string;
  fromLocation: string;
  toLocation: string;
  kilometers: number;
  vehicle: string;
  driver: string;
  ratePerKm: number;
  amountBasis: number;
  rateBasis: string;
  rateSource: string | null;
  notes: string | null;
  createdAt: string;
};

/** One month's mileage totals — drives the "Sum pr. periode"-card on the view. */
export type MileageMonthRow = {
  /** 1–12. */
  month: number;
  /** Danish month abbreviation (`jan`, `feb`, …). */
  label: string;
  /** Number of trips registered in the month. */
  tripCount: number;
  /** Sum of kilometres driven in the month. */
  kilometers: number;
  /** Sum of `amountBasis` in the month (kroner). */
  amountBasis: number;
};

export type CompanyMileage = {
  slug: string;
  selectedYear: string;
  archived: boolean;
  company: StatementCompanyBlock;
  fiscalYears: FiscalYearEntry[];
  periodStart: string;
  periodEnd: string;
  entries: MileageEntryRow[];
  /** Sum across the period. */
  totalKilometers: number;
  totalAmountBasis: number;
  tripCount: number;
  /** Twelve rows, jan…dec, including months with zero trips. */
  months: MileageMonthRow[];
};

function mapCoreMileageRow(row: CoreMileageEntryRow): MileageEntryRow {
  return {
    id: row.id,
    entryNo: row.entryNo,
    tripDate: row.tripDate,
    purpose: row.purpose,
    fromLocation: row.fromLocation,
    toLocation: row.toLocation,
    kilometers: row.kilometers,
    vehicle: row.vehicle,
    driver: row.driver,
    ratePerKm: row.ratePerKm,
    amountBasis: row.amountBasis,
    rateBasis: row.rateBasis,
    rateSource: row.rateSource,
    notes: row.notes,
    createdAt: row.createdAt,
  };
}

/**
 * Kørsel — every mileage entry whose `trip_date` falls within the selected
 * fiscal year, with a per-month summary. Re-uses the SAME
 * `buildMileagePeriodReport` core function the CLI's `mileage report` command
 * uses; the cockpit only opens the ledger and shapes the JSON. The mileage
 * log is documentation/audit data — never a journal posting — so an archived
 * year correctly returns an empty list (the report covers the live ledger).
 */
export function buildCompanyMileage(
  workspaceRoot: string,
  slug: string,
  year: number | null,
): CompanyMileage {
  const ctx = resolveStatementContext(workspaceRoot, slug, year);
  try {
    const companyBlock = statementCompanyBlock(ctx.company);
    const yearNum = parseInt(ctx.selectedLabel, 10);
    const yearStart = `${yearNum}-01-01`;
    const yearEnd = `${yearNum}-12-31`;

    if (ctx.isArchivedOnly) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: true,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: yearStart,
        periodEnd: yearEnd,
        entries: [],
        totalKilometers: 0,
        totalAmountBasis: 0,
        tripCount: 0,
        months: buildEmptyMileageMonths(),
      };
    }

    const report = buildMileagePeriodReport(ctx.db, {
      from: yearStart,
      to: yearEnd,
    });
    // The core only rejects an invalid range (we just constructed a valid one)
    // — any other failure would surface as a thrown error from `openDb`, not a
    // `{ok:false}`. So `report.ok` is true here in practice; we still guard.
    if (!report.ok) {
      return {
        slug: ctx.entry.slug,
        selectedYear: ctx.selectedLabel,
        archived: false,
        company: companyBlock,
        fiscalYears: ctx.years,
        periodStart: yearStart,
        periodEnd: yearEnd,
        entries: [],
        totalKilometers: 0,
        totalAmountBasis: 0,
        tripCount: 0,
        months: buildEmptyMileageMonths(),
      };
    }

    // Newest trip first — the most recent activity is what the user wants.
    const entries = report.entries
      .map(mapCoreMileageRow)
      .sort((a, b) => {
        if (a.tripDate !== b.tripDate) return b.tripDate.localeCompare(a.tripDate);
        return b.id - a.id;
      });

    const months = buildEmptyMileageMonths();
    for (const e of entries) {
      const monthIndex = parseInt(e.tripDate.slice(5, 7), 10) - 1;
      if (monthIndex < 0 || monthIndex > 11) continue;
      const m = months[monthIndex];
      if (!m) continue;
      m.tripCount += 1;
      m.kilometers = roundKroner(m.kilometers + e.kilometers);
      m.amountBasis = roundKroner(m.amountBasis + e.amountBasis);
    }

    return {
      slug: ctx.entry.slug,
      selectedYear: ctx.selectedLabel,
      archived: false,
      company: companyBlock,
      fiscalYears: ctx.years,
      periodStart: yearStart,
      periodEnd: yearEnd,
      entries,
      totalKilometers: roundKroner(report.totalKilometers),
      totalAmountBasis: roundKroner(report.totalAmountBasis),
      tripCount: entries.length,
      months,
    };
  } finally {
    ctx.db.close();
  }
}

function buildEmptyMileageMonths(): MileageMonthRow[] {
  return MONTH_NAMES_DK.map((label, i) => ({
    month: i + 1,
    label,
    tripCount: 0,
    kilometers: 0,
    amountBasis: 0,
  }));
}
