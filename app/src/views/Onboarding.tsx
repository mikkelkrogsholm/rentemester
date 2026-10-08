import * as stylex from "@stylexjs/stylex";
import { PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
// First-run onboarding. Shown when the workspace has no companies yet — it
// explains the cockpit's scope (overview + write actions: invoicing, bank CSV
// import, document intake) and presents the create-company form.

import { CompanyForm } from "../components/CompanyForm";

export function Onboarding({
	onCreated,
}: {
	onCreated: (slug: string) => void;
}) {
	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader title="Velkommen til Rentemester">
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
						Du har endnu ikke oprettet nogen virksomheder. Opret din første for
						at komme i gang.
					</p>
				</div>
			</PageHeader>

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
					viewStyles.site0,
				)}
			>
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
						viewStyles.site1,
					)}
				>
					Cockpittet er dit kontrolpanel: du opretter og overvåger virksomheder
					her. Du kan også bogføre direkte i cockpittet — udstede fakturaer,
					importere bankudtog (CSV) og indlæse bilag. Et par mere tekniske
					opgaver — fx lønhåndtering, opsætning af backup-destinationer og
					start-saldi ved migrering — håndteres stadig via kommandolinjen indtil
					videre.
				</p>
			</div>

			<CompanyForm
				onCreated={onCreated}
				submitLabel="Opret første virksomhed"
			/>
		</section>
	);
}

const viewStyles = stylex.create({
	site0: { marginBottom: 20 },
	site1: { marginTop: 0 },
});
