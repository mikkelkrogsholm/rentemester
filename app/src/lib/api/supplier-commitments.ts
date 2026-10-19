import type {
  CommitmentInput,
  CommitmentProposal,
  SupplierCommitmentsResponse,
} from "../types/commitments";
import { type ReadRequestOptions, request } from "./_shared";

const path = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/supplier-commitments`;
export const supplierCommitmentsApi = {
  supplierCommitments: (slug: string, startDate: string, options?: ReadRequestOptions) =>
    request<SupplierCommitmentsResponse>(
      `${path(slug)}?startDate=${encodeURIComponent(startDate)}`,
      options,
    ).then((r) => r.supplierCommitments),
  supplierCommitmentPlan: (slug: string, commitment: CommitmentInput) =>
    request<{ ok: true; plan: CommitmentProposal }>(`${path(slug)}/plan`, {
      method: "POST",
      readOnly: true,
      body: JSON.stringify({ commitment }),
    }).then((r) => r.plan),
  supplierCommitmentApply: (
    slug: string,
    input: { commitment: CommitmentInput; payloadHash: string; idempotencyKey: string },
  ) =>
    request(`${path(slug)}`, { method: "POST", body: JSON.stringify({ ...input, confirm: true }) }),
  supplierCommitmentChange: (
    slug: string,
    input: {
      commitmentId: string;
      action: "paused" | "ended" | "superseded";
      reason: string;
      expectedPayloadHash: string;
    },
  ) =>
    request(`${path(slug)}/change`, {
      method: "POST",
      body: JSON.stringify({ ...input, confirm: true }),
    }),
  supplierCommitmentMatch: (
    slug: string,
    input: {
      commitmentId: string;
      occurrenceDate: string;
      evidence: { kind: "canonical_document" | "payable" | "bank_transaction"; id: string };
    },
  ) =>
    request(`${path(slug)}/match`, {
      method: "POST",
      body: JSON.stringify({ ...input, confirm: true }),
    }),
};
