// Income statement, balance, trial balance, journal — list views + exports.

import type { ServerConfig } from "../config";
import { ApiError } from "../errors";
import {
  buildCompanyBalance,
  buildCompanyIncomeStatement,
  buildCompanyJournal,
  buildCompanyTrialBalance,
  resolveYearParam,
} from "../data";
import {
  exportBalanceCsv,
  exportBalancePdf,
  exportIncomeStatementCsv,
  exportIncomeStatementPdf,
  exportJournalCsv,
  exportTrialBalanceCsv,
  exportTrialBalancePdf,
  exportVatPdf,
  type StatementCsvExport,
} from "../data/statement-exports";
import { okResponse } from "./_shared";

export function handleCompanyIncomeStatement(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyIncomeStatement(config.workspaceRoot, slug, year);
  return okResponse({ incomeStatement: data });
}

export function handleCompanyBalance(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyBalance(config.workspaceRoot, slug, year);
  return okResponse({ balance: data });
}

export function handleCompanyTrialBalance(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const data = buildCompanyTrialBalance(config.workspaceRoot, slug, year);
  return okResponse({ trialBalance: data });
}

/**
 * GET /api/companies/:slug/(income-statement|balance|trial-balance)/export
 * — #372: cockpittet skal kunne hente de tre kerne-rapporter som CSV-filer
 * uden at gå via "Print hele browser-siden". Endpointet returnerer en
 * UTF-8-BOM-CSV med stabile danske kolonnenavne (header + metadata-blok), så
 * filen åbner direkte i Excel/Numbers/Sheets.
 *
 * Kun `format=csv` er understøttet i denne første slice — PDF-eksporten
 * kræver en ny render-pipeline og er bevidst udskudt til et opfølger-issue.
 * Et fravær eller `format=csv` accepteres som CSV; alt andet (fx `pdf`,
 * `xlsx`) afvises som en venlig 400 så cockpittet kan vise en hint i
 * stedet for at silent-fejle.
 */
export function handleCompanyStatementExport(
  config: ServerConfig,
  slug: string,
  url: URL,
  kind: "income-statement" | "balance" | "trial-balance",
): Response {
  const format = (url.searchParams.get("format") ?? "csv").toLowerCase();
  if (format !== "csv" && format !== "pdf") {
    throw ApiError.badRequest(
      `format=${format} understøttes ikke — kun csv og pdf er gyldige.`,
    );
  }
  const year = resolveYearParam(url.searchParams.get("year"));
  if (format === "pdf") {
    // #463 — PDF-slice. Deterministisk Helvetica/WinAnsi PDF uden
    // browser-print-chrome; samme tal som CSV-eksporten.
    let pdfExport: { content: Buffer; filename: string };
    if (kind === "income-statement") {
      pdfExport = exportIncomeStatementPdf(config.workspaceRoot, slug, year);
    } else if (kind === "balance") {
      pdfExport = exportBalancePdf(config.workspaceRoot, slug, year);
    } else {
      pdfExport = exportTrialBalancePdf(config.workspaceRoot, slug, year);
    }
    return new Response(pdfExport.content, {
      headers: {
        "content-type": "application/pdf",
        "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(pdfExport.filename)}`,
        "x-content-type-options": "nosniff",
        "cache-control": "private, no-store",
      },
    });
  }
  let exported: StatementCsvExport;
  if (kind === "income-statement") {
    exported = exportIncomeStatementCsv(config.workspaceRoot, slug, year);
  } else if (kind === "balance") {
    exported = exportBalanceCsv(config.workspaceRoot, slug, year);
  } else {
    exported = exportTrialBalanceCsv(config.workspaceRoot, slug, year);
  }
  return new Response(exported.content, {
    headers: {
      // text/csv per RFC 4180; charset=utf-8 fordi vi præfikser med en BOM.
      "content-type": "text/csv; charset=utf-8",
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(exported.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

/**
 * GET /api/companies/:slug/journal/export
 *
 * #465 — Posteringer (kassekladde) som CSV-download. Samme mønster som
 * statement-eksporterne (#372/#462): kun `format=csv` understøttes; et
 * valgfrit `account=<kontonr>` filtrerer til drilldown på en konto.
 */
/**
 * GET /api/companies/:slug/vat/export?format=pdf — Moms-rapport som PDF
 * (#464). Kun PDF understøttes; CSV-eksport af Moms er ikke en separat
 * use case — momsangivelsens form er stabil og bedst som PDF.
 */
export function handleCompanyVatExport(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const format = (url.searchParams.get("format") ?? "pdf").toLowerCase();
  if (format !== "pdf") {
    throw ApiError.badRequest(
      `format=${format} understøttes ikke — kun pdf er gyldig for moms-eksport.`,
    );
  }
  const year = resolveYearParam(url.searchParams.get("year"));
  const exported = exportVatPdf(config.workspaceRoot, slug, year);
  return new Response(exported.content, {
    headers: {
      "content-type": "application/pdf",
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(exported.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

export function handleCompanyJournalExport(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const format = (url.searchParams.get("format") ?? "csv").toLowerCase();
  if (format !== "csv") {
    throw ApiError.badRequest(
      `format=${format} understøttes ikke — kun csv er gyldig for posteringer-eksport.`,
    );
  }
  const year = resolveYearParam(url.searchParams.get("year"));
  const account = url.searchParams.get("account");
  const exported = exportJournalCsv(config.workspaceRoot, slug, year, account);
  return new Response(exported.content, {
    headers: {
      "content-type": "text/csv; charset=utf-8",
      "content-disposition": `attachment; filename*=UTF-8''${encodeURIComponent(exported.filename)}`,
      "x-content-type-options": "nosniff",
      "cache-control": "private, no-store",
    },
  });
}

export function handleCompanyJournal(
  config: ServerConfig,
  slug: string,
  url: URL,
): Response {
  const year = resolveYearParam(url.searchParams.get("year"));
  const account = url.searchParams.get("account");
  const data = buildCompanyJournal(config.workspaceRoot, slug, year, account);
  return okResponse({ journal: data });
}
