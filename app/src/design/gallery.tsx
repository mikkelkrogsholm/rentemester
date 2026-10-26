import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { createRoot } from "react-dom/client";
import { MemoryRouter } from "react-router-dom";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import {
	Amount,
	Button,
	DataTable,
	Dialog,
	EntitySelect,
	Field,
	MoneyInput,
	PageHeader,
	Pagination,
	ResultReceipt,
	Select,
	StatusBadge,
	Textarea,
} from "../components/ui";
import { cockpitStyles } from "./cockpit.stylex";
import { initializeDocumentStyle } from "./document-style";

initializeDocumentStyle();

function Gallery() {
	const [dialog, setDialog] = useState(false);
	const [amount, setAmount] = useState("1.234,56");
	const [entity, setEntity] = useState("");
	const [page, setPage] = useState(1),
		[pageSize, setPageSize] = useState(50);
	return (
		<main
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.gallery,
			)}
		>
			<PageHeader
				title="Komponentgalleri"
				description="Syntetiske eksempler. Samme komponenter, tilstande og skrifter som cockpit-appen."
			/>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.gallerySection,
				)}
			>
				<h2
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Handlinger
				</h2>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.rowActions,
					)}
				>
					{(["primary", "secondary", "quiet", "danger"] as const).map(
						(variant) => (
							<Button
								key={variant}
								variant={variant}
								xstyle={[cockpitStyles.buttonComposition]}
							>
								{variant}
							</Button>
						),
					)}
					<Button disabled xstyle={[cockpitStyles.buttonComposition]}>
						Utilgængelig
					</Button>
					<Button busy xstyle={[cockpitStyles.buttonComposition]}>
						Arbejder…
					</Button>
					<Button
						onClick={() => setDialog(true)}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Åbn dialog
					</Button>
				</div>
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.gallerySection,
				)}
			>
				<h2
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Formular og fejl
				</h2>
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.galleryFields,
					)}
				>
					<Field
						label="Beløb"
						help="Indtast med dansk tusind- og decimalseparator."
					>
						<MoneyInput value={amount} onValueChange={setAmount} />
					</Field>
					<Field label="Regnskabsår">
						<Select xstyle={[cockpitStyles.selectComposition]}>
							<option
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								2026
							</option>
							<option
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								2025 (arkiv)
							</option>
						</Select>
					</Field>
					<Field
						label="Begrundelse"
						error="Angiv begrundelsen, før du fortsætter."
					>
						<Textarea rows={3} xstyle={[cockpitStyles.textareaComposition]} />
					</Field>
					<EntitySelect
						label="Konto"
						value={entity}
						onChange={setEntity}
						options={[
							{ id: "a", label: "Syntetisk omsætningskonto" },
							{ id: "b", label: "Syntetisk udgiftskonto" },
						]}
					/>
				</div>
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.gallerySection,
				)}
			>
				<h2
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Beløb og status
				</h2>
				<DataTable
					caption="Syntetiske regnskabsværdier"
					rowKey={(row) => row.id}
					rows={[
						{ id: 1, value: 1234567890.12, currency: "DKK" },
						{ id: 2, value: -1234.56, currency: "EUR" },
						{ id: 3, value: 0, currency: "DKK" },
						{ id: 4, value: null, currency: "DKK" },
					]}
					columns={[
						{
							id: "amount",
							label: "Beløb",
							render: (row) => (
								<Amount value={row.value} currency={row.currency} />
							),
						},
						{
							id: "state",
							label: "Status",
							render: (row) => (
								<StatusBadge
									label={row.value === null ? "Ukendt" : "Kontrolleret"}
									tone={row.value === null ? "warning" : "success"}
								/>
							),
						},
					]}
				/>
				<Pagination
					total={125}
					page={page}
					pageSize={pageSize}
					onPageChange={setPage}
					onPageSizeChange={(size) => {
						setPageSize(size);
						setPage(1);
					}}
				/>
			</section>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.gallerySection,
				)}
			>
				<h2
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h2,
					)}
				>
					Tilstande
				</h2>
				<Loading />
				<ErrorState message="Serveren kunne ikke nås." onRetry={() => {}} />
				<Banner kind="warning">Afventer serverens bekræftelse.</Banner>
				<ResultReceipt title="Handlingen er gennemført">
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Den syntetiske post er registreret. Kvitteringen viser serverens
						resultat.
					</p>
				</ResultReceipt>
			</section>
			{dialog && (
				<Dialog
					title="Gennemgå handling"
					onClose={() => setDialog(false)}
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Kontrollér oplysningerne, før du fortsætter.
					</p>
					<Button
						variant="secondary"
						onClick={() => setDialog(false)}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Annullér
					</Button>
					<Button
						onClick={() => setDialog(false)}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Bekræft
					</Button>
				</Dialog>
			)}
		</main>
	);
}
const root = document.getElementById("root");
if (!root) throw new Error("Gallery root missing");
createRoot(root).render(
	<MemoryRouter>
		<Gallery />
	</MemoryRouter>,
);
