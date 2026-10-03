import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import { Button, Input, PageHeader, Select } from "../components/ui";
import { FormEvent, useState } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth-context";
import { useAsync } from "../lib/useAsync";
import type { WorkspaceInvitationInput, WorkspaceMember } from "../lib/types";

const roleLabels = {
  owner: "Ejer", bookkeeper: "Bogholder", reviewer: "Reviewer", reader: "Læseadgang",
} as const;

type PendingChange =
  | { kind: "workspace-role"; userId: string; role: WorkspaceMember["workspaceRole"] }
  | { kind: "disable"; userId: string }
  | { kind: "company-role"; userId: string; companySlug: string; role: WorkspaceInvitationInput["companyRole"] }
  | { kind: "company-revoke"; userId: string; companySlug: string };

export function WorkspaceAccessView() {
  const { context } = useAuth();
  const invitationState = useAsync((signal) => api.workspaceInvitations({ signal }), []);
  const memberState = useAsync((signal) => api.workspaceMembers({ signal }), []);
  const ownedCompanies = context?.companies.filter((company) => !company.archived && company.role === "owner") ?? [];
  const firstCompany = ownedCompanies[0]?.slug ?? "";
  const [input, setInput] = useState<WorkspaceInvitationInput>({
    email: "", workspaceRole: "member", companySlug: firstCompany, companyRole: "bookkeeper",
  });
  const [selectedUserId, setSelectedUserId] = useState("");
  const [selectedCompanySlug, setSelectedCompanySlug] = useState(firstCompany);
  const [workspaceRole, setWorkspaceRole] = useState<WorkspaceMember["workspaceRole"]>("member");
  const [companyRole, setCompanyRole] = useState<WorkspaceInvitationInput["companyRole"]>("bookkeeper");
  const [pending, setPending] = useState<PendingChange | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const outcome = useMutationOutcome(() => { invitationState.reload(); memberState.reload(); });
  useUnsavedChanges(Boolean(input.email));
  if ((invitationState.loading && !invitationState.data) || (memberState.loading && !memberState.data)) return <Loading label="Henter adgang…" />;
  if ((invitationState.error && !invitationState.data) || (memberState.error && !memberState.data)) {
    return <ErrorState message="Adgangen kunne ikke hentes." onRetry={() => {
      invitationState.reload(); memberState.reload();
    }} />;
  }

  const members = memberState.data!;
  const selectedMember = members.find((member) => member.userId === selectedUserId) ?? null;
  const reload = () => { invitationState.reload(); memberState.reload(); };

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (outcome.isBlocked()) return; setBusy(true); setError(null); setMessage(null);
    try {
      await outcome.run(() => api.createWorkspaceInvitation(input));
      setInput((current) => ({ ...current, email: "" }));
      setMessage("Invitationen er sendt."); reload();
    } catch { setError("Invitationen kunne ikke sendes."); }
    finally { setBusy(false); }
  }

  async function cancel(invitationId: string) {
    if (outcome.isBlocked()) return;
    setBusy(true); setError(null); setMessage(null);
    try {
      await outcome.run(() => api.cancelWorkspaceInvitation(invitationId));
      setMessage("Invitationen er annulleret."); reload();
    } catch { setError("Invitationen kunne ikke annulleres."); }
    finally { setBusy(false); }
  }

  async function applyPending() {
    if (!pending || outcome.isBlocked()) return;
    if (pending.kind === "workspace-role") {
      await outcome.run(() => api.updateWorkspaceMemberAccess({
        action: "set-role", userId: pending.userId, workspaceRole: pending.role,
      }));
    } else if (pending.kind === "disable") {
      await outcome.run(() => api.updateWorkspaceMemberAccess({ action: "disable", userId: pending.userId }));
    } else if (pending.kind === "company-role") {
      await outcome.run(() => api.updateWorkspaceMemberCompany({
        action: "grant", userId: pending.userId,
        companySlug: pending.companySlug, role: pending.role,
      }));
    } else {
      await outcome.run(() => api.updateWorkspaceMemberCompany({
        action: "revoke", userId: pending.userId, companySlug: pending.companySlug,
      }));
    }
    setMessage("Adgangen er opdateret."); setError(null); reload();
  }

  return <section>
    {outcome.feedback}
    {(invitationState.error || memberState.error) && <Banner kind="warning">Status kunne ikke opdateres. Formularen er bevaret; de tidligere hentede adgangsoplysninger vises fortsat.</Banner>}
    <PageHeader title="Brugere og adgang"><div><p className="muted">Invitér og administrér kun de virksomheder, hvor du selv er ejer. Adgang kræver verificeret e-mail og MFA.</p></div></PageHeader>
    {message && <Banner kind="success">{message}</Banner>}
    {error && <Banner kind="error">{error}</Banner>}
    <section className="card">
      <h3>Aktive brugere</h3>
      {members.length === 0 ? <p className="muted">Ingen aktive brugere.</p> : <div className="table-wrap"><div className="table-scroll"><table><thead><tr><th>Bruger</th><th>Workspace</th><th>Virksomhedsadgang</th><th>Sikkerhed</th></tr></thead><tbody>{members.map((member) => <tr key={member.userId}><td>{member.name}<br /><span className="muted">{member.email}</span></td><td>{member.workspaceRole === "workspace_owner" ? "Ejer" : "Medlem"}</td><td>{member.memberships.length === 0 ? "—" : member.memberships.map((membership) => `${membership.companyName}: ${roleLabels[membership.role]}`).join(", ")}</td><td>{member.accessReady ? "Klar" : "Afventer e-mail/MFA"}</td></tr>)}</tbody></table></div></div>}
    </section>
    <section className="card">
      <h3>Ændr adgang</h3>
      <p className="muted">Vælg én bruger og én ændring ad gangen. Sidste aktive ejer kan ikke fjernes.</p>
      <label>Bruger<Select disabled={outcome.blocked} value={selectedUserId} onChange={(event) => {
        const userId = event.target.value; setSelectedUserId(userId);
        const member = members.find((candidate) => candidate.userId === userId);
        if (member) setWorkspaceRole(member.workspaceRole);
      }}><option value="">Vælg bruger</option>{members.map((member) => <option key={member.userId} value={member.userId}>{member.name} — {member.email}</option>)}</Select></label>
      {selectedMember && <>
        <label>Workspace-rolle<Select disabled={outcome.blocked} value={workspaceRole} onChange={(event) => setWorkspaceRole(event.target.value as WorkspaceMember["workspaceRole"])}><option value="member">Medlem</option><option value="workspace_owner">Ejer</option></Select></label>
        <div className="modal-actions"><Button disabled={outcome.blocked} requiredPermission="workspace.members.manage" variant="secondary" type="button" className="btn secondary" onClick={() => setPending({ kind: "workspace-role", userId: selectedMember.userId, role: workspaceRole })}>Gem workspace-rolle</Button><Button disabled={outcome.blocked} requiredPermission="workspace.members.manage" variant="danger" type="button" className="btn danger" onClick={() => setPending({ kind: "disable", userId: selectedMember.userId })}>Deaktivér bruger</Button></div>
        {ownedCompanies.length > 0 && <>
          <label>Virksomhed<Select disabled={outcome.blocked} value={selectedCompanySlug} onChange={(event) => setSelectedCompanySlug(event.target.value)}>{ownedCompanies.map((company) => <option key={company.slug} value={company.slug}>{company.name}</option>)}</Select></label>
          <label>Virksomhedsrolle<Select disabled={outcome.blocked} value={companyRole} onChange={(event) => setCompanyRole(event.target.value as WorkspaceInvitationInput["companyRole"])}>{Object.entries(roleLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></label>
          <div className="modal-actions"><Button disabled={outcome.blocked} requiredPermission="workspace.members.manage" variant="secondary" type="button" className="btn secondary" onClick={() => setPending({ kind: "company-role", userId: selectedMember.userId, companySlug: selectedCompanySlug, role: companyRole })}>Gem virksomhedsrolle</Button><Button disabled={outcome.blocked} requiredPermission="workspace.members.manage" variant="danger" type="button" className="btn danger" onClick={() => setPending({ kind: "company-revoke", userId: selectedMember.userId, companySlug: selectedCompanySlug })}>Fjern virksomhedsadgang</Button></div>
        </>}
      </>}
    </section>
    <section className="card">
      <h3>Ny invitation</h3>
      <form onSubmit={submit}>
        <label>E-mail<Input disabled={outcome.blocked} type="email" autoComplete="email" value={input.email} onChange={(event) => setInput({ ...input, email: event.target.value })} required /></label>
        <label>Virksomhed<Select disabled={outcome.blocked} value={input.companySlug} onChange={(event) => setInput({ ...input, companySlug: event.target.value })} required>{ownedCompanies.map((company) => <option key={company.slug} value={company.slug}>{company.name}</option>)}</Select></label>
        <label>Rolle<Select disabled={outcome.blocked} value={input.companyRole} onChange={(event) => {
          const selectedRole = event.target.value as WorkspaceInvitationInput["companyRole"];
          setInput({ ...input, companyRole: selectedRole, workspaceRole: selectedRole === "owner" ? "workspace_owner" : "member" });
        }}>{Object.entries(roleLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></label>
        <Button requiredPermission="workspace.members.manage" className="btn" type="submit" disabled={outcome.blocked || (busy || !input.companySlug)}>Send invitation</Button>
      </form>
    </section>
    <section className="card">
      <h3>Invitationer</h3>
      {invitationState.data!.length === 0 ? <p className="muted">Ingen invitationer endnu.</p> : <div className="table-wrap"><div className="table-scroll"><table><thead><tr><th>E-mail</th><th>Virksomhed</th><th>Rolle</th><th>Udløber</th><th>Status</th><th></th></tr></thead><tbody>{invitationState.data!.map((invitation) => <tr key={invitation.invitationId}><td>{invitation.email}</td><td>{invitation.companySlug}</td><td>{roleLabels[invitation.companyRole]}</td><td>{new Date(invitation.expiresAt).toLocaleDateString("da-DK")}</td><td>{invitation.status}</td><td>{(invitation.status === "issued" || invitation.status === "delivery_confirmed") && <Button requiredPermission="workspace.members.manage" variant="secondary" className="btn secondary" type="button" disabled={outcome.blocked || (busy)} onClick={() => void cancel(invitation.invitationId)}>Annullér</Button>}</td></tr>)}</tbody></table></div></div>}
    </section>
    {pending && <ConfirmDialog
      title="Bekræft adgangsændring"
      body={<p>Ændringen registreres i det append-only revisionsspor og træder i kraft med det samme.</p>}
      confirmLabel="Gennemfør ændring"
      confirmKind={pending.kind === "disable" || pending.kind === "company-revoke" ? "danger" : "primary"}
      onConfirm={applyPending}
      onClose={() => setPending(null)} onRefresh={reload}
    />}
  </section>;
}
