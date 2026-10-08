import * as stylex from "@stylexjs/stylex";
import { ButtonLink, PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// Standalone "add company" route — the same form as onboarding, reached from
// the portfolio when the workspace is already populated.

import { useNavigate } from "react-router-dom";
import { CompanyForm } from "../components/CompanyForm";

export function AddCompanyView() {
	const navigate = useNavigate();
	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader
				title="Tilføj virksomhed"
				actions={
					<>
						<ButtonLink
							to="/"
							variant={"secondary"}
							xstyle={[cockpitStyles.aComposition]}
						>
							Annullér
						</ButtonLink>
					</>
				}
			>
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
						Registrerer en ny virksomhed i arbejdsområdet og opretter dens
						regnskab.
					</p>
				</div>
			</PageHeader>
			<CompanyForm onCreated={(slug) => navigate(`/companies/${slug}`)} />
		</section>
	);
}
