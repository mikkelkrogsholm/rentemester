import * as stylex from "@stylexjs/stylex";
import { PageHeader } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";

// Hjælp og support — cockpittets ene synlige exit-vej for en bruger der står
// fast (#421). Top-baren peger hertil; siden samler links til www-sitets
// dokumentation, kontaktformular, GitHub-issues og en kort kom-i-gang-tjekliste.
//
// Vi holder siden bevidst statisk og uden API-kald: hjælp skal kunne nås selv
// hvis backend-API'et er nede eller bruger lige har installeret cockpittet og
// ikke kan tolke fejlmeddelelser. Indhold er på dansk og uden CLI-jargon
// (acceptkriterium fra #421).

const DOCS_BASE = "https://rentemester.dk";
const REPO_ISSUES = "https://github.com/mikkelkrogsholm/rentemester/issues";

type ExternalLinkProps = {
	href: string;
	children: React.ReactNode;
};

function ExternalLink({ href, children }: ExternalLinkProps) {
	return (
		<a
			href={href}
			target="_blank"
			rel="noopener noreferrer"
			{...stylex.props(cockpitStyles.aComposition)}
		>
			{children}
		</a>
	);
}

export function HelpView() {
	return (
		<section
			{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
		>
			<PageHeader title="Hjælp og support"></PageHeader>

			<p
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.muted,
				)}
			>
				Står du fast? Her er en samlet vej til dokumentation, kontakt og
				fejlrapportering. Linkene åbner i ny fane.
			</p>

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.helpViewHelpGrid,
				)}
			>
				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.helpViewHelpGridCard,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.helpViewHelpGridCardH3,
						)}
					>
						Dokumentation
					</h3>
					<ul
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.helpViewHelpGridUl,
						)}
					>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							<ExternalLink href={`${DOCS_BASE}/saadan-virker-det`}>
								Sådan virker det
							</ExternalLink>{" "}
							— kort intro til hvordan Rentemester bogfører for dig.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							<ExternalLink href={`${DOCS_BASE}/funktioner`}>
								Funktioner
							</ExternalLink>{" "}
							— overblik over hvad cockpittet kan.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							<ExternalLink href={`${DOCS_BASE}/docs/installation`}>
								Brugerguide
							</ExternalLink>{" "}
							— kom-i-gang-vejledning fra installation til første postering.
						</li>
					</ul>
				</article>

				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.helpViewHelpGridCard,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.helpViewHelpGridCardH3,
						)}
					>
						Kom i gang
					</h3>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						Fem trin fra installeret cockpit til moms:
					</p>
					<ol
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.helpViewHelpGridOl,
						)}
					>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							Opret virksomheden under “Tilføj virksomhed”.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							Importér kontoudtog under “Bank”.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							Indlæs og bogfør bilag under “Bilag”.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							Afstem bank-bevægelser under “Bank”.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							Generér momsangivelse under “Moms” når perioden er slut.
						</li>
					</ol>
					<p
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.muted,
						)}
					>
						“Posteringer” er en visning over alt der allerede er bogført — ikke
						et bogføringssted.
					</p>
				</article>

				<article
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.card,
						cockpitStyles.helpViewHelpGridCard,
					)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.helpViewHelpGridCardH3,
						)}
					>
						Kontakt og support
					</h3>
					<ul
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.helpViewHelpGridUl,
						)}
					>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							<ExternalLink href={`${DOCS_BASE}/kontakt`}>Kontakt</ExternalLink>{" "}
							— skriv direkte til teamet bag Rentemester.
						</li>
						<li
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.helpViewHelpGridLiNotFirstChild,
							)}
						>
							<ExternalLink href={REPO_ISSUES}>
								Rapportér en fejl på GitHub
							</ExternalLink>{" "}
							— åbn et issue hvis cockpittet driller.
						</li>
					</ul>
				</article>
			</div>
		</section>
	);
}
