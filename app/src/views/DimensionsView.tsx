import * as stylex from "@stylexjs/stylex";
import { FormEvent, useState } from "react";
import { useParams } from "react-router-dom";
import { PageState, ResponsiveTable } from "../components/CockpitPrimitives";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { Banner } from "../components/Feedback";
import { Button, Input, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";

type Pending = {
	create?: "definition" | "member";
	title: string;
	body: string;
	noteLabel?: string;
	noteInitialValue?: string;
	run: (note: string) => Promise<unknown>;
};

/** Small review surface: all changes are append-only and are made through the
 * reviewed API flows; this screen intentionally makes history visible. */
export function DimensionsView() {
	const { slug = "" } = useParams();
	const definitions = useAsync(
		(signal) => api.dimensionDefinitions(slug, { signal }),
		[slug],
	);
	const members = useAsync(
		(signal) => api.dimensionMembers(slug, undefined, { signal }),
		[slug],
	);
	const [pending, setPending] = useState<Pending | null>(null);
	const [message, setMessage] = useState<string | null>(null);
	const [definition, setDefinition] = useState({
		dimensionId: "",
		kind: "",
		name: "",
	});
	const [member, setMember] = useState({
		dimensionId: "",
		memberId: "",
		name: "",
	});
	const reload = () => {
		definitions.reload();
		members.reload();
	};
	const outcome = useMutationOutcome(reload);
	const markDefinitionSaved = useUnsavedChanges(
		Boolean(definition.dimensionId || definition.kind || definition.name),
	);
	const markMemberSaved = useUnsavedChanges(
		Boolean(member.dimensionId || member.memberId || member.name),
	);
	const queue = (
		title: string,
		body: string,
		run: (note: string) => Promise<unknown>,
		noteLabel?: string,
		noteInitialValue?: string,
		create?: Pending["create"],
	) => {
		if (!outcome.isBlocked())
			setPending({ title, body, run, noteLabel, noteInitialValue, create });
	};
	const confirm = async (note: string) => {
		if (!pending || outcome.isBlocked()) return;
		await outcome.run(() => pending.run(note));
		if (pending.create === "definition") {
			setDefinition({ dimensionId: "", kind: "", name: "" });
			markDefinitionSaved();
		}
		if (pending.create === "member") {
			setMember({ dimensionId: "", memberId: "", name: "" });
			markMemberSaved();
		}
		setPending(null);
		setMessage("Ændringen er registreret i revisionssporet.");
		definitions.reload();
		members.reload();
	};
	const addDefinition = (event: FormEvent) => {
		event.preventDefault();
		if (!definition.dimensionId || !definition.kind || !definition.name) return;
		queue(
			"Opret dimension",
			"Definitionen oprettes som en append-only hændelse. Eksisterende bogføring ændres ikke.",
			() => api.createDimensionDefinition(slug, definition),
			undefined,
			undefined,
			"definition",
		);
	};
	const addMember = (event: FormEvent) => {
		event.preventDefault();
		if (!member.dimensionId || !member.memberId || !member.name) return;
		queue(
			"Opret medlem",
			"Medlemmet oprettes som en append-only hændelse. Det kan efterfølgende aktiveres, deaktiveres eller omdøbes med historik.",
			() => api.createDimensionMember(slug, member),
			undefined,
			undefined,
			"member",
		);
	};
	if (
		(definitions.loading && !definitions.data) ||
		(members.loading && !members.data)
	)
		return <PageState kind="loading" title="Henter dimensioner" />;
	if (
		(definitions.error && !definitions.data) ||
		(members.error && !members.data)
	)
		return (
			<PageState
				kind="error"
				title="Dimensioner kunne ikke hentes"
				onRetry={() => {
					definitions.reload();
					members.reload();
				}}
			>
				{definitions.error ?? members.error}
			</PageState>
		);
	return (
		<section
			data-cockpit-page="dimensions"
			data-evidence-issue="655"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.statement,
			)}
		>
			{outcome.feedback}
			<PageHeader title="Dimensioner">
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
						Brug dimensioner til at samle bogføring efter fx projekt eller
						afdeling.
					</p>
				</div>
			</PageHeader>
			{message && <Banner kind="success">{message}</Banner>}
			<div
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
					Opret dimension
				</h3>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Start med den inddeling, du vil bruge i den daglige bogføring.
				</p>
				<form
					onSubmit={addDefinition}
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					<Input
						disabled={outcome.blocked}
						aria-label="Dimensions-id"
						value={definition.dimensionId}
						onChange={(e) =>
							setDefinition({ ...definition, dimensionId: e.target.value })
						}
						placeholder="fx projekt"
						required
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>
					<Input
						disabled={outcome.blocked}
						aria-label="Dimensionstype"
						value={definition.kind}
						onChange={(e) =>
							setDefinition({ ...definition, kind: e.target.value })
						}
						placeholder="fx projekt"
						required
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>
					<Input
						disabled={outcome.blocked}
						aria-label="Dimensionsnavn"
						value={definition.name}
						onChange={(e) =>
							setDefinition({ ...definition, name: e.target.value })
						}
						placeholder="fx Projekter"
						required
						xstyle={[cockpitStyles.rowActionsInputComposition2]}
					/>
					<Button
						disabled={outcome.blocked}
						requiredPermission="company.master-data"
						type="submit"
						xstyle={[cockpitStyles.statementBtnComposition]}
					>
						Opret dimension
					</Button>
				</form>
			</div>
			<details
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<summary {...stylex.props(cockpitStyles.summaryComposition)}>
					Avanceret: definitioner, medlemmer og historik
				</summary>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Definitioner
				</h3>
				<ResponsiveTable
					label="Dimensionsdefinitioner"
					xstyle={[cockpitStyles.tableDataComposition]}
				>
					<thead
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableThead,
						)}
					>
						<tr
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTr,
							)}
						>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Id
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Navn
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Status
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Hændelse
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Handling
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableTbody,
						)}
					>
						{definitions.data!.length ? (
							definitions.data!.map((row) => (
								<tr
									key={row.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}
								>
									<td
										data-label="Id"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.dimension_id}
									</td>
									<td
										data-label="Navn"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.name}
									</td>
									<td
										data-label="Status"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.status}
									</td>
									<td
										data-label="Hændelse"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.event_type}
									</td>
									<td
										data-label="Handling"
										{...stylex.props(cockpitStyles.rowActionsComposition)}
									>
										<Button
											disabled={outcome.blocked}
											requiredPermission="company.master-data"
											type="button"
											onClick={() =>
												queue(
													"Omdøb dimension",
													`Det nye navn bliver en append-only revision af ${row.dimension_id}.`,
													(name) =>
														api.changeDimensionDefinition(slug, {
															dimensionId: row.dimension_id,
															action: "rename",
															name,
														}),
													"Nyt navn",
													row.name,
												)
											}
											variant={"secondary"}
											xstyle={[cockpitStyles.statementBtnComposition]}
										>
											Omdøb
										</Button>
										<Button
											disabled={outcome.blocked}
											requiredPermission="company.master-data"
											type="button"
											onClick={() =>
												queue(
													row.status === "active"
														? "Deaktiver dimension"
														: "Aktivér dimension",
													`Statusændringen gemmes med historik for ${row.dimension_id}.`,
													() =>
														api.changeDimensionDefinition(slug, {
															dimensionId: row.dimension_id,
															action:
																row.status === "active"
																	? "deactivate"
																	: "activate",
														}),
												)
											}
											variant={"secondary"}
											xstyle={[cockpitStyles.statementBtnComposition]}
										>
											{row.status === "active" ? "Deaktiver" : "Aktivér"}
										</Button>
									</td>
								</tr>
							))
						) : (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td
									colSpan={5}
									{...stylex.props(cockpitStyles.tableDataTdComposition)}
								>
									Ingen definitioner endnu.
								</td>
							</tr>
						)}
					</tbody>
				</ResponsiveTable>
				<div
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
						Opret medlem
					</h3>
					<form
						onSubmit={addMember}
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.rowActions,
						)}
					>
						<Input
							disabled={outcome.blocked}
							aria-label="Medlemmets dimension"
							value={member.dimensionId}
							onChange={(e) =>
								setMember({ ...member, dimensionId: e.target.value })
							}
							placeholder="dimensions-id"
							required
							xstyle={[cockpitStyles.rowActionsInputComposition2]}
						/>
						<Input
							disabled={outcome.blocked}
							aria-label="Medlems-id"
							value={member.memberId}
							onChange={(e) =>
								setMember({ ...member, memberId: e.target.value })
							}
							placeholder="fx projekt-a"
							required
							xstyle={[cockpitStyles.rowActionsInputComposition2]}
						/>
						<Input
							disabled={outcome.blocked}
							aria-label="Medlemsnavn"
							value={member.name}
							onChange={(e) => setMember({ ...member, name: e.target.value })}
							placeholder="fx Projekt A"
							required
							xstyle={[cockpitStyles.rowActionsInputComposition2]}
						/>
						<Button
							disabled={outcome.blocked}
							requiredPermission="company.master-data"
							type="submit"
							xstyle={[cockpitStyles.statementBtnComposition]}
						>
							Opret medlem
						</Button>
					</form>
				</div>
				<h3
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h3,
					)}
				>
					Medlemmer
				</h3>
				<ResponsiveTable
					label="Dimensionsmedlemmer"
					xstyle={[cockpitStyles.tableDataComposition]}
				>
					<thead
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableThead,
						)}
					>
						<tr
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.responsiveTableTr,
							)}
						>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Dimension
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Id
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Navn
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Status
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Hændelse
							</th>
							<th
								scope="col"
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTh,
									cockpitStyles.responsiveTableThLastChild,
									cockpitStyles.tableDataTh,
								)}
							>
								Handling
							</th>
						</tr>
					</thead>
					<tbody
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.responsiveTableTbody,
						)}
					>
						{members.data!.length ? (
							members.data!.map((row) => (
								<tr
									key={row.id}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.responsiveTableTr,
									)}
								>
									<td
										data-label="Dimension"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.dimension_id}
									</td>
									<td
										data-label="Id"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.member_id}
									</td>
									<td
										data-label="Navn"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.name}
									</td>
									<td
										data-label="Status"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.status}
									</td>
									<td
										data-label="Hændelse"
										{...stylex.props(cockpitStyles.tableDataTdComposition2)}
									>
										{row.event_type}
									</td>
									<td
										data-label="Handling"
										{...stylex.props(cockpitStyles.rowActionsComposition)}
									>
										<Button
											disabled={outcome.blocked}
											requiredPermission="company.master-data"
											type="button"
											onClick={() =>
												queue(
													"Omdøb medlem",
													`Det nye navn bliver en append-only revision af ${row.member_id}.`,
													(name) =>
														api.changeDimensionMember(slug, {
															dimensionId: row.dimension_id,
															memberId: row.member_id!,
															action: "rename",
															name,
														}),
													"Nyt navn",
													row.name,
												)
											}
											variant={"secondary"}
											xstyle={[cockpitStyles.statementBtnComposition]}
										>
											Omdøb
										</Button>
										<Button
											disabled={outcome.blocked}
											requiredPermission="company.master-data"
											type="button"
											onClick={() =>
												queue(
													row.status === "active"
														? "Deaktiver medlem"
														: "Aktivér medlem",
													`Statusændringen gemmes med historik for ${row.member_id}.`,
													() =>
														api.changeDimensionMember(slug, {
															dimensionId: row.dimension_id,
															memberId: row.member_id!,
															action:
																row.status === "active"
																	? "deactivate"
																	: "activate",
														}),
												)
											}
											variant={"secondary"}
											xstyle={[cockpitStyles.statementBtnComposition]}
										>
											{row.status === "active" ? "Deaktiver" : "Aktivér"}
										</Button>
									</td>
								</tr>
							))
						) : (
							<tr
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.responsiveTableTr,
								)}
							>
								<td
									colSpan={6}
									{...stylex.props(cockpitStyles.tableDataTdComposition)}
								>
									Ingen medlemmer endnu.
								</td>
							</tr>
						)}
					</tbody>
				</ResponsiveTable>
			</details>
			{pending && (
				<ConfirmDialog
					title={pending.title}
					body={
						<p
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							{pending.body}
						</p>
					}
					confirmLabel="Bekræft ændring"
					noteLabel={pending.noteLabel}
					noteInitialValue={pending.noteInitialValue}
					onConfirm={confirm}
					onRefresh={reload}
					onClose={() => setPending(null)}
				/>
			)}
		</section>
	);
}
