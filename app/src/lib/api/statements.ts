import type {
  BalanceResponse,
  IncomeStatementResponse,
  JournalResponse,
  TrialBalanceResponse,
} from "../types";
import type { ReportSource } from "../types/common";
import { ApiError, request, type ReadRequestOptions } from "./_shared";

export function checkedReportSource<T extends {archived:boolean;selectedSource?:ReportSource}>(body:T,source?:ReportSource):T {
  if (!body || source && (body.selectedSource ?? (body.archived ? "archive" : "live")) !== source) throw new ApiError("internal","Rapportens kilde svarer ikke til det valgte år og kilde. Der vises ingen alternativ rapport.",200);
  return body;
}
export const statementsApi = {
  incomeStatement: (slug: string, year?: string, options?: ReadRequestOptions & {source?:ReportSource}) =>
    request<IncomeStatementResponse>(
      `/api/companies/${encodeURIComponent(slug)}/income-statement${
        year ? `?year=${encodeURIComponent(year)}${options?.source ? `&source=${encodeURIComponent(options.source)}` : ""}` : options?.source ? `?source=${encodeURIComponent(options.source)}` : ""
      }`,
      options ? {signal:options.signal} : undefined,
    ).then((r) => checkedReportSource(r.incomeStatement,options?.source)),

  balance: (slug: string, year?: string, options?: ReadRequestOptions & {source?:ReportSource}) =>
    request<BalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/balance${
        year ? `?year=${encodeURIComponent(year)}${options?.source ? `&source=${encodeURIComponent(options.source)}` : ""}` : options?.source ? `?source=${encodeURIComponent(options.source)}` : ""
      }`,
      options ? {signal:options.signal} : undefined,
    ).then((r) => checkedReportSource(r.balance,options?.source)),

  trialBalance: (slug: string, year?: string, options?: ReadRequestOptions & {source?:ReportSource}) =>
    request<TrialBalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/trial-balance${
        year ? `?year=${encodeURIComponent(year)}${options?.source ? `&source=${encodeURIComponent(options.source)}` : ""}` : options?.source ? `?source=${encodeURIComponent(options.source)}` : ""
      }`,
      options ? {signal:options.signal} : undefined,
    ).then((r) => checkedReportSource(r.trialBalance,options?.source)),

  journal: (slug: string, year?: string, account?: string, options?: ReadRequestOptions & {source?:ReportSource}) => {
    const params = new URLSearchParams();
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    if (options?.source) params.set("source", options.source);
    const query = params.toString();
    return request<JournalResponse>(
      `/api/companies/${encodeURIComponent(slug)}/journal${
        query ? `?${query}` : ""
      }`,
      options ? {signal:options.signal} : undefined,
    ).then((r) => checkedReportSource(r.journal,options?.source));
  },

  /**
   * URL of the Resultatopgørelse, Balance eller Saldobalance som CSV-fil
   * (#372). Endpointet returnerer en `text/csv`-attachment med stabile
   * danske kolonnenavne og semikolon-separator, så browseren downloader
   * filen direkte når URL'en åbnes (typisk via et `<a href download>`).
   * PDF følger i et opfølger-issue.
   */
  statementCsvUrl: (
    slug: string,
    report: "income-statement" | "balance" | "trial-balance",
    year?: string,
    source?: ReportSource,
  ) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
    if (source) params.set("source", source);
    return `/api/companies/${encodeURIComponent(slug)}/${report}/export?${params.toString()}`;
  },

  /**
   * #463 — Statement PDF download URL. Samme endpoint som CSV, bare med
   * format=pdf. Deterministisk Helvetica/WinAnsi PDF.
   */
  statementPdfUrl: (
    slug: string,
    report: "income-statement" | "balance" | "trial-balance",
    year?: string,
    source?: ReportSource,
  ) => {
    const params = new URLSearchParams({ format: "pdf" });
    if (year) params.set("year", year);
    if (source) params.set("source", source);
    return `/api/companies/${encodeURIComponent(slug)}/${report}/export?${params.toString()}`;
  },

  /**
   * Posteringer (kassekladde) som CSV-download (#465). Samme deterministiske
   * UTF-8 BOM + semikolon-CSV som de tre kerne-rapporter; et valgfrit
   * `account` filtrerer drilldown'et til netop den konto.
   */
  journalPdfUrl: (slug:string, year?:string, account?:string|null, source?:ReportSource) => {
    const params=new URLSearchParams({format:"pdf"});if(year)params.set("year",year);if(account)params.set("account",account);if(source)params.set("source",source);
    return `/api/companies/${encodeURIComponent(slug)}/journal/export?${params.toString()}`;
  },
  journalCsvUrl: (slug: string, year?: string, account?: string | null, source?: ReportSource) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    if (source) params.set("source", source);
    return `/api/companies/${encodeURIComponent(slug)}/journal/export?${params.toString()}`;
  },
};
