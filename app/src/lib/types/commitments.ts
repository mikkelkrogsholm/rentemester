export type CommitmentInput = {
  commitmentId: string;
  vendorPartyId: string;
  type: string;
  description: string;
  businessPurpose: string;
  amount: number;
  currency: string;
  frequency: "weekly" | "monthly" | "quarterly" | "yearly";
  nextDate: string;
  startDate?: string;
  renewalDate?: string;
  noticeDate?: string;
  endDate?: string;
  evidenceRefs: string[];
  vatProposal?: string;
};
export type CommitmentProposal = CommitmentInput & {
  payloadHash: string;
  previousPayloadHash: string | null;
  vendorSnapshot: string;
  schedule: Array<{
    date: string;
    amount: number;
    currency: string;
  }>;
  warnings: string[];
};
export type SupplierCommitmentRow = {
  commitmentId: string;
  payloadHash: string;
  createdAt: string;
  vendor: string;
  purpose: string;
  amount: number | null;
  currency: string | null;
  frequency: string | null;
  nextDate: string | null;
  renewalDate: string | null;
  status: string;
  evidenceRefs: string[];
  missingDocument: boolean;
  proposal?: CommitmentProposal;
};
export type CommitmentEvidence = {
  kind: "canonical_document" | "payable" | "bank_transaction";
  id: string;
  date: string;
  amount: number;
  currency: string;
  label: string;
};
export type SupplierCommitmentsResponse = {
  ok: true;
  supplierCommitments: {
    slug: string;
    company: {
      name: string;
      currency: string;
    };
    fiscalYears: Array<{
      label: string;
      start: string | null;
      end: string | null;
      source: "live" | "archive";
    }>;
    selectedYear: string;
    startDate: string;
    vendors: Array<{
      id: number;
      name: string;
    }>;
    documents: Array<{
      id: number;
      documentNo: string | null;
      filename: string | null;
    }>;
    evidenceOptions: CommitmentEvidence[];
    occurrences: Array<{
      commitmentId: string;
      date: string;
      amount: number;
      currency: string;
      payloadHash: string;
    }>;
    history: Array<{
      commitmentId: string;
      event: string;
      payloadHash: string;
      createdAt: string;
      actor?: string;
      principal?: string;
      payload: {
        reason?: string;
      };
    }>;
    commitments: SupplierCommitmentRow[];
    alerts: Array<{
      commitmentId: string;
      kind: string;
      date: string;
      severity: string;
    }>;
    matches: Array<{
      commitmentId: string;
      occurrenceDate: string;
      variance: {
        amount: number | null;
        dateDays: number;
        late: boolean;
        currency: {
          status: string;
        };
        party?: {
          status: string;
        };
        documentation: string;
      };
    }>;
    forecast: {
      ok: boolean;
      openingCash: number | null;
      lowestPoint: number | null;
      currency: string;
      openingBalanceVerified?: boolean;
      periods: Array<{
        weekStart: string;
        openingCash: number | null;
        receivables: number;
        payables: number;
        commitments: number;
        budgets: number;
        undatedBudgetAssumptions: number;
        scenarios: number;
        obligations: number;
        intercompany: number;
        closingCash: number | null;
        scenarioClosingCash: number | null;
        excluded: Array<{
          source: string;
          amount: number;
          currency: string;
          reason?: string;
        }>;
        sources: Array<{
          source: string;
          amount: number;
          reference: string;
          assumption?: boolean;
          settlementStatus?: "unknown" | null;
        }>;
      }>;
      completeness: {
        status?: string;
        included: string[];
        excluded: string[];
      };
    };
  };
};