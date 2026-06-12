import type {
  BalanceResponse,
  IncomeStatementResponse,
  JournalResponse,
  TrialBalanceResponse,
} from "../types";
import { request } from "./_shared";

export const statementsApi = {
  incomeStatement: (slug: string, year?: string) =>
    request<IncomeStatementResponse>(
      `/api/companies/${encodeURIComponent(slug)}/income-statement${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.incomeStatement),

  balance: (slug: string, year?: string) =>
    request<BalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/balance${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.balance),

  trialBalance: (slug: string, year?: string) =>
    request<TrialBalanceResponse>(
      `/api/companies/${encodeURIComponent(slug)}/trial-balance${
        year ? `?year=${encodeURIComponent(year)}` : ""
      }`,
    ).then((r) => r.trialBalance),

  journal: (slug: string, year?: string, account?: string) => {
    const params = new URLSearchParams();
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    const query = params.toString();
    return request<JournalResponse>(
      `/api/companies/${encodeURIComponent(slug)}/journal${
        query ? `?${query}` : ""
      }`,
    ).then((r) => r.journal);
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
  ) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
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
  ) => {
    const params = new URLSearchParams({ format: "pdf" });
    if (year) params.set("year", year);
    return `/api/companies/${encodeURIComponent(slug)}/${report}/export?${params.toString()}`;
  },

  /**
   * Posteringer (kassekladde) som CSV-download (#465). Samme deterministiske
   * UTF-8 BOM + semikolon-CSV som de tre kerne-rapporter; et valgfrit
   * `account` filtrerer drilldown'et til netop den konto.
   */
  journalCsvUrl: (slug: string, year?: string, account?: string | null) => {
    const params = new URLSearchParams({ format: "csv" });
    if (year) params.set("year", year);
    if (account) params.set("account", account);
    return `/api/companies/${encodeURIComponent(slug)}/journal/export?${params.toString()}`;
  },
};
