// #338 — Annual report (regnskabsklasse-B) builder.

export type AnnualReportLine = {
  accountNo: string;
  name: string;
  amount: number;
};

export type AnnualReportSection = {
  total: number;
  lines: AnnualReportLine[];
};

export type AnnualReport = {
  ok: boolean;
  fiscalYearStart: string;
  fiscalYearEnd: string;
  company: {
    name: string;
    cvr: string | null;
    country: string;
    currency: string;
  };
  /** Profit & loss summary tied to the same fiscal year. */
  profitAndLoss?: {
    income: AnnualReportSection;
    expense: AnnualReportSection;
    result: number;
  };
  balanceSheet?: {
    assets: AnnualReportSection;
    liabilities: AnnualReportSection;
    equity: AnnualReportSection;
  };
  notes?: Array<{ id: string; title: string; body: string }>;
  ledelsespategning?: { date: string; body: string };
  errors: string[];
};

export type CompanyAnnualReportResponse = {
  ok: true;
  annualReport: {
    slug: string;
    company: {
      name: string;
      cvr: string | null;
      country: string;
      currency: string;
      fiscalYearStartMonth: number | string;
      fiscalYearLabelStrategy: string;
    };
    fiscalYearStart: string;
    fiscalYearEnd: string;
    report: AnnualReport;
  };
};
