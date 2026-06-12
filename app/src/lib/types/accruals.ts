// #337 — Periodisering / accrual register.

export type AccrualRegisterRow = {
  accrualId: number;
  accrualType: "prepaid_expense" | "accrued_expense" | "deferred_revenue";
  description: string;
  totalAmount: number;
  recognitionPeriods: number;
  recognizedPeriods: number;
  recognizedAmount: number;
  remainingAmount: number;
  fullyRecognized: boolean;
  balanceAccountNo: string;
  resultAccountNo: string;
  firstRecognitionDate: string;
  periodStepMonths: number;
};

export type AccrualRegisterReport = {
  ok: boolean;
  accruals: AccrualRegisterRow[];
  totals: { totalAmount: number; recognizedAmount: number; remainingAmount: number };
  errors: string[];
};

export type CompanyAccrualsResponse = {
  ok: true;
  accruals: {
    slug: string;
    company: { name: string; cvr: string | null; country: string; currency: string };
    report: AccrualRegisterReport;
  };
};
