import { request, type ReadRequestOptions } from "./_shared";
export type ServicePrincipal = {
  serviceAccountId: string; displayName: string; createdAt: string; activeCredentialCount: number;
  credentials: Array<{ credentialId: string; enabled: boolean | null; expiresAt: string | null }>;
};
export type ServiceReceipt = {
  ok: true; operationId: string; serviceAccountId?: string; credentialId?: string;
  secret?: string; revoked?: boolean; status?: string; recovered?: boolean;
};
export type ServiceAction = "create" | "rotate" | "revoke" | "recover";
export const servicePrincipalsApi = {
  async list(options?: ReadRequestOptions) {
    return (await request<{ ok: true; principals: ServicePrincipal[] }>("/api/workspace/service-principals", options)).principals;
  },
  mutate(action: ServiceAction, body: Record<string, unknown>) {
    return request<ServiceReceipt>(`/api/workspace/service-principals${action === "create" ? "" : `/${action}`}`, {
      method: "POST", body: JSON.stringify(body),
    });
  },
};
