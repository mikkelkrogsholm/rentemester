import { request } from "./_shared";
export type ScheduledOccurrence = { periodIndex: number; issueDate: string };
export type ScheduledRestoreConsent = {
  required: boolean; boundaryHash: string; historicalOwnerUserId: string | null;
  historicalQueued: ScheduledOccurrence[];
};
export type ScheduledRow = {
  templateId: number; templateHash: string; stateHash: string;
  state: "disabled" | "enabled" | "paused" | "cancelled";
  ownerUserId: string | null; lastRun: Record<string, unknown> | null;
  nextOccurrence: ScheduledOccurrence | null; missedCount: number;
  queued: ScheduledOccurrence[]; error: string | null; history: Record<string, unknown>[];
  executionMode?: "app" | "hosted";
  hostedConsent?: { active: boolean; serviceAccountId: string; credentialId: string; grantorUserId: string; consentHash: string; authorityHash: string; policy: "normal_due_only" } | null;
  restoreConsent?: ScheduledRestoreConsent | null;
};
export type ScheduledPlan = {
  template: { id: number; name: string; interval: string; intervalCount: number; firstIssueDate: string; paymentTermsDays: number; deliveryPeriodMode: string; payload: { currency?: string | null; buyer?: { name?: string; address?: string; vatOrCvr?: string } | null; seller?: { name?: string; address?: string; vatOrCvr?: string } | null; vatRatePercent?: number | null; lines: Array<{ description: string; quantity: number; unitPriceExVat: number }> } };
  templateId: number; templateHash: string; stateHash: string; asOfDate: string;
  normalDue: ScheduledOccurrence | null; missed: ScheduledOccurrence[];
  missedCount: number; truncated: boolean; queued: ScheduledOccurrence[]; planHash: string;
  restoreConsent?: ScheduledRestoreConsent | null;
};
export type HostedScheduledPlan = {
  templateId: number; templateHash: string; stateHash: string; template: ScheduledPlan["template"];
  schedule: ScheduledPlan; executor: { serviceAccountId: string; credentialId: string; displayName: string };
  authorityHash: string; policy: "normal_due_only"; planHash: string;
};
export type ScheduledAction = "enable" | "pause" | "resume" | "cancel" | "catch-up" | "skip" | "dispatch" | "reconsent";
const path = (slug: string) => `/api/companies/${encodeURIComponent(slug)}/scheduled-recurring`;
export const scheduledRecurringApi = {
  async current(slug: string, signal?: AbortSignal) {
    return (await request<{ ok: true; scheduledRecurring: {
      asOfDate: string; timeZone: string; localService: false; rows: ScheduledRow[];
    } }>(path(slug), { signal })).scheduledRecurring;
  },
  async plan(slug: string, templateId: number) {
    return (await request<{ ok: true; plan: ScheduledPlan }>(`${path(slug)}/${encodeURIComponent(String(templateId))}/plan`)).plan;
  },
  async hostedPlan(slug: string, templateId: number) {
    return (await request<{ ok: true; hostedPlan: HostedScheduledPlan }>(`${path(slug)}/${encodeURIComponent(String(templateId))}/hosted-plan`)).hostedPlan;
  },
  mutateHosted(slug: string, action: "hosted-consent" | "hosted-revoke", body: Record<string, unknown>) {
    return request<{ ok: true; scheduledRecurring: ScheduledRow }>(`${path(slug)}/${action}`, { method: "POST", body: JSON.stringify(body) });
  },
  mutate(slug: string, action: ScheduledAction, body: Record<string, unknown>) {
    return request<{ ok: true; scheduledRecurring: ScheduledRow; generation?: unknown;
      dispatch?: { status: "succeeded" | "failed"; error?: string } }>(`${path(slug)}/${action}`, {
      method: "POST", body: JSON.stringify(body),
    });
  },
};
