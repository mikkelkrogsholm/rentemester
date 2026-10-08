import * as stylex from "@stylexjs/stylex";
import { FormEvent, useState } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { Button, Input, PageHeader, Select } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { useAuth } from "../lib/auth-context";
import type { WorkspaceInvitationInput, WorkspaceMember } from "../lib/types";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

const roleLabels = {
	owner: "Ejer",
	bookkeeper: "Bogholder",
	reviewer: "Reviewer",
	reader: "Læseadgang",
} as const;

type PendingChange =
	| {
			kind: "workspace-role";
			userId: string;
			role: WorkspaceMember["workspaceRole"];
	  }
	| { kind: "disable"; userId: string }
	| {
			kind: "company-role";
			userId: string;
			companySlug: string;
			role: WorkspaceInvitationInput["companyRole"];
	  }
	| { kind: "company-revoke"; userId: string; companySlug: string };

export function WorkspaceAccessView() {
	const { context } = useAuth();
	const invitationState = useAsync(
		(signal) => api.workspaceInvitations({ signal }),
		[],
	);
	const memberState = useAsync(
		(signal) => api.workspaceMembers({ signal }),
		[],
	);
	const ownedCompanies =
		context?.companies.filter(
			(company) => !company.archived && company.role === "owner",
		) ?? [];
	const firstCompany = ownedCompanies[0]?.slug ?? "";
	const [input, setInput] = useState<WorkspaceInvitationInput>({
		email: "",
		workspaceRole: "member",
		companySlug: firstCompany,
		companyRole: "bookkeeper",
	});
	const [selectedUserId, setSelectedUserId] = useState("");
	const [selectedCompanySlug, setSelectedCompanySlug] = useState(firstCompany);
	const [workspaceRole, setWorkspaceRole] =
		useState<WorkspaceMember["workspaceRole"]>("member");
	const [companyRole, setCompanyRole] =
		useState<WorkspaceInvitationInput["companyRole"]>("bookkeeper");
	const [pending, setPending] = useState<PendingChange | null>(null);
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState<string | null>(null);
	const [error, setError] = useState<string | null>(null);

	const outcome = useMutationOutcome(() => {
		invitationState.reload();
		memberState.reload();
	});
	useUnsavedChanges(Boolean(input.email));
	if (
		(invitationState.loading && !invitationState.data) ||
		(memberState.loading && !memberState.data)
	)
		return <Loading label="Henter adgang…" />;
	if (
		(invitationState.error && !invitationState.data) ||
		(memberState.error && !memberState.data)
	) {
		return (
			<ErrorState
				message="Adgangen kunne ikke hentes."
				onRetry={() => {
					invitationState.reload();
					memberState.reload();
				}}
			/>
		);
	}

	const members = memberState.data!;
	const selectedMember =
		members.find((member) => member.userId === selectedUserId) ?? null;
	const reload = () => {
		invitationState.reload();
		memberState.reload();
	};

	async function submit(event: FormEvent) {
		event.preventDefault();
		if (outcome.isBlocked()) return;
		setBusy(true);
		setError(null);
		setMessage(null);
		try {
			await outcome.run(() => api.createWorkspaceInvitation(input));
			setInput((current) => ({ ...current, email: "" }));
			setMessage("Invitationen er sendt.");
			reload();
		} catch {
			setError("Invitationen kunne ikke sendes.");
		} finally {
			setBusy(false);
		}
	}

	async function cancel(invitationId: string) {
		if (outcome.isBlocked()) return;
		setBusy(true);
		setError(null);
		setMessage(null);
		try {
			await outcome.run(() => api.cancelWorkspaceInvitation(invitationId));
			setMessage("Invitationen er annulleret.");
			reload();
		} catch {
			setError("Invitationen kunne ikke annulleres.");
		} finally {
			setBusy(false);
		}
	}

	async function applyPending() {
		if (!pending || outcome.isBlocked()) return;
		if (pending.kind === "workspace-role") {
			await outcome.run(() =>
				api.updateWorkspaceMemberAccess({
					action: "set-role",
					userId: pending.userId,
					workspaceRole: pending.role,
				}),
			);
		} else if (pending.kind === "disable") {
			await outcome.run(() =>
				api.updateWorkspaceMemberAccess({
					action: "disable",
					userId: pending.userId,
				}),
			);
		} else if (pending.kind === "company-role") {
			await outcome.run(() =>
				api.updateWorkspaceMemberCompany({
					action: "grant",
					userId: pending.userId,
					companySlug: pending.companySlug,
					role: pending.role,
				}),
			);
		} else {
			await outcome.run(() =>
				api.updateWorkspaceMemberCompany({
					action: "revoke",
					userId: pending.userId,
					companySlug: pending.companySlug,
				}),
			);
		}
		setMessage("Adgangen er opdateret.");
		setError(null);
		reload();
	}

	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			{outcome.feedback}
			{(invitationState.error || memberState.error) && (
				<Banner kind="warning">
					Status kunne ikke opdateres. Formularen er bevaret; de tidligere
					hentede adgangsoplysninger vises fortsat.
				</Banner>
			)}
			<PageHeader title="Brugere og adgang">
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Invitér og administrér kun de virksomheder, hvor du selv er ejer.
						Adgang kræver verificeret e-mail og MFA.
					</p>
				</div>
			</PageHeader>
			{message && <Banner kind="success">{message}</Banner>}
			{error && <Banner kind="error">{error}</Banner>}
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Aktive brugere
				</h3>
				{members.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen aktive brugere.
					</p>
				) : (
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.tableWrap,
						)}
					>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.tableScroll,
							)}
							data-ui="table-scroll"
						>
							<table
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<thead
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<tr
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Bruger
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Workspace
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Virksomhedsadgang
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Sikkerhed
										</th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{members.map((member) => (
										<tr
											key={member.userId}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{member.name}
												<br
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
													)}
												/>
												<span
													{...stylex.props(
														cockpitStyles.element,
														cockpitStyles.focusVisible,
														cockpitStyles.muted,
													)}
												>
													{member.email}
												</span>
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{member.workspaceRole === "workspace_owner"
													? "Ejer"
													: "Medlem"}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{member.memberships.length === 0
													? "—"
													: member.memberships
															.map(
																(membership) =>
																	`${membership.companyName}: ${roleLabels[membership.role]}`,
															)
															.join(", ")}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{member.accessReady ? "Klar" : "Afventer e-mail/MFA"}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</div>
				)}
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Ændr adgang
				</h3>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					Vælg én bruger og én ændring ad gangen. Sidste aktive ejer kan ikke
					fjernes.
				</p>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Bruger
					<Select
						disabled={outcome.blocked}
						value={selectedUserId}
						onChange={(event) => {
							const userId = event.target.value;
							setSelectedUserId(userId);
							const member = members.find(
								(candidate) => candidate.userId === userId,
							);
							if (member) setWorkspaceRole(member.workspaceRole);
						}}
						xstyle={[cockpitStyles.selectComposition]}
					>
						<option
							value=""
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Vælg bruger
						</option>
						{members.map((member) => (
							<option
								key={member.userId}
								value={member.userId}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								{member.name} — {member.email}
							</option>
						))}
					</Select>
				</label>
				{selectedMember && (
					<>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Workspace-rolle
							<Select
								disabled={outcome.blocked}
								value={workspaceRole}
								onChange={(event) =>
									setWorkspaceRole(
										event.target.value as WorkspaceMember["workspaceRole"],
									)
								}
								xstyle={[cockpitStyles.selectComposition]}
							>
								<option
									value="member"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Medlem
								</option>
								<option
									value="workspace_owner"
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Ejer
								</option>
							</Select>
						</label>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.modalActions,
							)}
						>
							<Button
								disabled={outcome.blocked}
								requiredPermission="workspace.members.manage"
								variant="secondary"
								type="button"
								onClick={() =>
									setPending({
										kind: "workspace-role",
										userId: selectedMember.userId,
										role: workspaceRole,
									})
								}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Gem workspace-rolle
							</Button>
							<Button
								disabled={outcome.blocked}
								requiredPermission="workspace.members.manage"
								variant="danger"
								type="button"
								onClick={() =>
									setPending({ kind: "disable", userId: selectedMember.userId })
								}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								Deaktivér bruger
							</Button>
						</div>
						{ownedCompanies.length > 0 && (
							<>
								<label
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Virksomhed
									<Select
										disabled={outcome.blocked}
										value={selectedCompanySlug}
										onChange={(event) =>
											setSelectedCompanySlug(event.target.value)
										}
										xstyle={[cockpitStyles.selectComposition]}
									>
										{ownedCompanies.map((company) => (
											<option
												key={company.slug}
												value={company.slug}
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{company.name}
											</option>
										))}
									</Select>
								</label>
								<label
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									Virksomhedsrolle
									<Select
										disabled={outcome.blocked}
										value={companyRole}
										onChange={(event) =>
											setCompanyRole(
												event.target
													.value as WorkspaceInvitationInput["companyRole"],
											)
										}
										xstyle={[cockpitStyles.selectComposition]}
									>
										{Object.entries(roleLabels).map(([value, label]) => (
											<option
												key={value}
												value={value}
												{...stylex.props(
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{label}
											</option>
										))}
									</Select>
								</label>
								<div
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.modalActions,
									)}
								>
									<Button
										disabled={outcome.blocked}
										requiredPermission="workspace.members.manage"
										variant="secondary"
										type="button"
										onClick={() =>
											setPending({
												kind: "company-role",
												userId: selectedMember.userId,
												companySlug: selectedCompanySlug,
												role: companyRole,
											})
										}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Gem virksomhedsrolle
									</Button>
									<Button
										disabled={outcome.blocked}
										requiredPermission="workspace.members.manage"
										variant="danger"
										type="button"
										onClick={() =>
											setPending({
												kind: "company-revoke",
												userId: selectedMember.userId,
												companySlug: selectedCompanySlug,
											})
										}
										xstyle={[cockpitStyles.buttonComposition]}
									>
										Fjern virksomhedsadgang
									</Button>
								</div>
							</>
						)}
					</>
				)}
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Ny invitation
				</h3>
				<form
					onSubmit={submit}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						E-mail
						<Input
							disabled={outcome.blocked}
							type="email"
							autoComplete="email"
							value={input.email}
							onChange={(event) =>
								setInput({ ...input, email: event.target.value })
							}
							required
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Virksomhed
						<Select
							disabled={outcome.blocked}
							value={input.companySlug}
							onChange={(event) =>
								setInput({ ...input, companySlug: event.target.value })
							}
							required
							xstyle={[cockpitStyles.selectComposition]}
						>
							{ownedCompanies.map((company) => (
								<option
									key={company.slug}
									value={company.slug}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{company.name}
								</option>
							))}
						</Select>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Rolle
						<Select
							disabled={outcome.blocked}
							value={input.companyRole}
							onChange={(event) => {
								const selectedRole = event.target
									.value as WorkspaceInvitationInput["companyRole"];
								setInput({
									...input,
									companyRole: selectedRole,
									workspaceRole:
										selectedRole === "owner" ? "workspace_owner" : "member",
								});
							}}
							xstyle={[cockpitStyles.selectComposition]}
						>
							{Object.entries(roleLabels).map(([value, label]) => (
								<option
									key={value}
									value={value}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{label}
								</option>
							))}
						</Select>
					</label>
					<Button
						requiredPermission="workspace.members.manage"
						type="submit"
						disabled={outcome.blocked || busy || !input.companySlug}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Send invitation
					</Button>
				</form>
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Invitationer
				</h3>
				{invitationState.data!.length === 0 ? (
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Ingen invitationer endnu.
					</p>
				) : (
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.tableWrap,
						)}
					>
						<div
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.tableScroll,
							)}
							data-ui="table-scroll"
						>
							<table
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								<thead
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									<tr
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											E-mail
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Virksomhed
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Rolle
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Udløber
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											Status
										</th>
										<th
											{...stylex.props(
												cockpitStyles.tableDataTh,
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										></th>
									</tr>
								</thead>
								<tbody
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
									)}
								>
									{invitationState.data!.map((invitation) => (
										<tr
											key={invitation.invitationId}
											{...stylex.props(
												cockpitStyles.element,
												cockpitStyles.focusVisible,
											)}
										>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{invitation.email}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{invitation.companySlug}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{roleLabels[invitation.companyRole]}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{new Date(invitation.expiresAt).toLocaleDateString(
													"da-DK",
												)}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{invitation.status}
											</td>
											<td
												{...stylex.props(
													cockpitStyles.tableDataTd,
													cockpitStyles.element,
													cockpitStyles.focusVisible,
												)}
											>
												{(invitation.status === "issued" ||
													invitation.status === "delivery_confirmed") && (
													<Button
														requiredPermission="workspace.members.manage"
														variant="secondary"
														type="button"
														disabled={outcome.blocked || busy}
														onClick={() => void cancel(invitation.invitationId)}
														xstyle={[cockpitStyles.buttonComposition]}
													>
														Annullér
													</Button>
												)}
											</td>
										</tr>
									))}
								</tbody>
							</table>
						</div>
					</div>
				)}
			</section>
			{pending && (
				<ConfirmDialog
					title="Bekræft adgangsændring"
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Ændringen registreres i det append-only revisionsspor og træder i
							kraft med det samme.
						</p>
					}
					confirmLabel="Gennemfør ændring"
					confirmKind={
						pending.kind === "disable" || pending.kind === "company-revoke"
							? "danger"
							: "primary"
					}
					onConfirm={applyPending}
					onClose={() => setPending(null)}
					onRefresh={reload}
				/>
			)}
		</section>
	);
}
