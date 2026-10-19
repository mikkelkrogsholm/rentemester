export type ArchiveSource = { fileName: string; contentBase64: string };
export type ArchiveImportPlan = {
  format: "rentemester-native-closed-history/1"; status: "ready"; planHash: string;
  source: { sha256: string; sizeBytes: number }; declaredCurrency: string;
  currencyDeclaration: "operator"; excludedLatestYear: number;
  years: Array<{ fiscalYear: number; postingCount: number; balanceCount: number; hasBalances: boolean; status: "new" | "retained" }>;
  limitations: string[];
};
export type ArchiveApply = ArchiveSource & { planHash: string; idempotencyKey: string; confirm: true };
export type ArchiveImportReceipt = {
  importId: number; sourceSha256: string; planHash: string; excludedLatestYear: number;
  declaredCurrency: string; years: Array<{ fiscalYear: number; postingCount: number; balanceCount: number }>;
};
export type ClosedArchiveYear = {
  fiscalYear: number; sourceSystem: "dinero"; postingCount: number; balanceCount: number;
  importedAt: string; importId: number; sourceHash: string; parserContract: string;
  declaredCurrency: string; hasBalances: boolean;
  postings?: Array<{ lineNo: number; accountNo: string; accountName: string; transactionDate: string; voucher: string; voucherType: string; text: string; vatType: string; amount: string; sourceAmount: string; runningBalance: string | null; sourceRunningBalance: string | null }>;
  balances?: Array<{ lineNo: number; accountNo: string; accountName: string; amount: string; sourceAmount: string }>;
};
