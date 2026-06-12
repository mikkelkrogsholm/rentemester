import type { CompanyAgentSuggestions } from "../../lib/types";
import { STATEMENT_COMPANY } from "./_shared";

/** Agent-forslag fixture (#346) — two pending suggestions across two types. */
export function agentSuggestions(
  over: Partial<CompanyAgentSuggestions> = {},
): CompanyAgentSuggestions {
  return {
    slug: "acme-aps",
    company: STATEMENT_COMPANY,
    rows: [
      {
        exceptionId: 101,
        type: "AGENT_PAYABLE_OVERDUE",
        kindLabel: "Overforfalden kreditorpost",
        severity: "high",
        rationale:
          "kreditorpost V-1001 til Software ApS på 1.250,00 kr. med forfald 2026-02-09 er overforfalden og endnu ikke betalt.",
        requiredAction:
          "Betal kreditorposten, og afstem den udgående bankbetaling mod den med 'payable pay'.",
        ruleId: "DK-PAYABLE-001",
        sourceEvidence: { payableId: 11, overdueDays: 104 },
        postingPreview: null,
        agentActor: "system:agent-loop",
        agentProgram: "exception.recorded",
        createdAt: "2026-05-01T08:30:00.000Z",
        relatedDocumentId: 101,
        relatedBankTransactionId: null,
        link: "leverandoerfaktura",
      },
      {
        exceptionId: 102,
        type: "AGENT_ACCRUAL_RECOGNITION_DUE",
        kindLabel: "Periodeafgrænsning klar til bogføring",
        severity: "medium",
        rationale:
          'Periodeafgrænsningspost "Forsikring 2026" — periode 4/12 (1.000,00 kr.) med planlagt bogføringsdato 2026-05-01 er forfalden og endnu ikke bogført.',
        requiredAction:
          "Bogfør periode 4 af periodeafgrænsningsposten med 'accrual recognize'.",
        ruleId: "DK-BOOKKEEPING-ACCRUAL-001",
        sourceEvidence: { accrualId: 7, periodIndex: 4 },
        postingPreview: null,
        agentActor: "system:agent-loop",
        agentProgram: "exception.recorded",
        createdAt: "2026-05-02T08:30:00.000Z",
        relatedDocumentId: null,
        relatedBankTransactionId: null,
        link: "posteringer",
      },
    ],
    count: 2,
    bySeverity: { high: 1, medium: 1, low: 0 },
    ...over,
  };
}
