import type {
  WorkspaceInvitation,
  WorkspaceInvitationClaim,
  WorkspaceInvitationInput,
  WorkspaceMember,
} from "../types";
import { request, type ReadRequestOptions } from "./_shared";

export const workspaceApi = {
  workspaceInvitations: (options?: ReadRequestOptions) =>
    request<{ ok: true; invitations: WorkspaceInvitation[] }>("/api/workspace/invitations", options)
      .then((result) => result.invitations),
  createWorkspaceInvitation: (input: WorkspaceInvitationInput) =>
    request<{ ok: true; invitation: WorkspaceInvitation }>("/api/workspace/invitations", {
      method: "POST",
      body: JSON.stringify(input),
    }).then((result) => result.invitation),
  cancelWorkspaceInvitation: (invitationId: string) =>
    request<{ ok: true; invitation: WorkspaceInvitation }>("/api/workspace/invitations/cancel", {
      method: "POST",
      body: JSON.stringify({ invitationId }),
    }).then((result) => result.invitation),
  claimWorkspaceInvitation: (input: WorkspaceInvitationClaim) =>
    request<{ ok: true; accepted: true; accessReady: boolean; nextStep: string }>(
      "/api/invitations/claim",
      { method: "POST", body: JSON.stringify(input) },
    ),
  workspaceMembers: (options?: ReadRequestOptions) =>
    request<{ ok: true; members: WorkspaceMember[] }>("/api/workspace/members", options)
      .then((result) => result.members),
  updateWorkspaceMemberAccess: (input: {
    userId: string;
    action: "set-role" | "disable";
    workspaceRole?: WorkspaceMember["workspaceRole"];
  }) => request<{ ok: true }>("/api/workspace/members/access", {
    method: "POST", body: JSON.stringify(input),
  }),
  updateWorkspaceMemberCompany: (input: {
    userId: string;
    companySlug: string;
    action: "grant" | "revoke";
    role?: WorkspaceInvitation["companyRole"];
  }) => request<{ ok: true }>("/api/workspace/members/company", {
    method: "POST", body: JSON.stringify(input),
  }),
};
