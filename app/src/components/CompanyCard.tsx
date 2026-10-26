import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
// One company in the portfolio overview. Renders the headline health an owner
// judges a company on — resultat, faktisk banksaldo, omsætning, moms (beløb +
// frist) and opgaver — plus the derived "needs attention" flags. The whole
// card is a link into the company's Overblik.

import { Link } from "react-router-dom";
import {
	attentionFlags,
	attentionLevel,
	formatDeadline,
	formatKroner,
} from "../lib/format";
import type { CompanySummary } from "../lib/types";

export function CompanyCard({ company }: { company: CompanySummary }) {
	const level = attentionLevel(company);
	const flags = attentionFlags(company);

	return (
		<article
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.companyCard,
				((company.archived && level === "critical") ||
					(!company.archived && level === "critical")) &&
					cockpitStyles.companyCardLevelCritical,
				((company.archived && level === "warning") ||
					(!company.archived && level === "warning")) &&
					cockpitStyles.companyCardLevelWarning,
				((company.archived && level === "ok") ||
					(!company.archived && level === "ok")) &&
					cockpitStyles.companyCardLevelOk,
				company.archived && cockpitStyles.companyCardArchived,
			)}
		>
			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.companyCardCcHead,
				)}
			>
				<div
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<h3
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.h3,
							cockpitStyles.companyCardH3,
						)}
					>
						<Link
							to={`/companies/${company.slug}`}
							{...stylex.props(cockpitStyles.aComposition)}
						>
							{company.name}
						</Link>
					</h3>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.companyCardCcCvr,
						)}
					>
						{company.cvr ? `CVR ${company.cvr}` : "CVR ikke angivet"}
						{company.fiscalYear ? ` · regnskabsår ${company.fiscalYear}` : ""}
					</div>
				</div>
				{company.archived && (
					<span
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.badge,
						)}
					>
						Arkiveret
					</span>
				)}
			</div>

			{company.ledgerMissing ? (
				<p
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.emptyInline,
					)}
				>
					Virksomheden er registreret, men har endnu intet regnskab på disken.
				</p>
			) : (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.ccMetrics,
					)}
				>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.ccMetric,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.ccMetricMLabel,
							)}
						>
							Resultat (år til dato)
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.mValue,
								cockpitStyles.ccMetricMValue,
								!(company.resultat < 0) && cockpitStyles.ccMetricPosMValue,
								company.resultat < 0 && cockpitStyles.ccMetricNegMValue,
							)}
						>
							{formatKroner(company.resultat)}
						</span>
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.ccMetric,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.ccMetricMLabel,
							)}
						>
							Faktisk banksaldo
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.mValue,
								cockpitStyles.ccMetricMValue,
							)}
						>
							{company.actualBankBalance === null
								? "—"
								: formatKroner(company.actualBankBalance)}
						</span>
						{company.actualBankBalance === null && (
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.ccMetricMSub,
								)}
							>
								{company.bankStatementStatus === "ambiguous"
									? "kontoudtoget kan ikke verificeres — se Bank"
									: company.bankStatementStatus === "no-balance-column"
										? "kontoudtoget har ingen saldo-kolonne"
										: "intet kontoudtog importeret"}
							</span>
						)}
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.ccMetric,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.ccMetricMLabel,
							)}
						>
							Omsætning
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.mValue,
								cockpitStyles.ccMetricMValue,
							)}
						>
							{formatKroner(company.omsaetning)}
						</span>
					</div>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.ccMetric,
						)}
					>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.ccMetricMLabel,
							)}
						>
							{company.vat && company.vat.payable < 0
								? "Moms til gode"
								: "Moms at betale"}
						</span>
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.mValue,
								cockpitStyles.ccMetricMValue,
							)}
						>
							{company.vat ? formatKroner(company.vat.payable) : "—"}
						</span>
						{company.vat && (
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									!(
										company.vat.payable > 0 && company.vat.daysRemaining <= 30
									) && cockpitStyles.ccMetricMSub,
								)}
							>
								frist {company.vat.deadline} ·{" "}
								{formatDeadline(company.vat.daysRemaining)}
							</span>
						)}
					</div>
				</div>
			)}

			{!company.ledgerMissing && (
				<div
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.ccTasks,
					)}
				>
					{company.attentionStatus === "clear" ? (
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.ccTaskNone,
							)}
						>
							Ingen åbne opgaver
						</span>
					) : (
						<>
							<span
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.ccTaskCount,
								)}
							>
								{company.openTaskCount} åbne opgaver
							</span>
							{company.taskGroups.slice(0, 2).map((g) => (
								<span
									key={g.type}
									{...stylex.props(
										cockpitStyles.element,
										cockpitStyles.focusVisible,
										cockpitStyles.ccTaskLine,
										g.severity === "high" && cockpitStyles.ccTaskLineSevHigh,
										g.severity === "medium" &&
											cockpitStyles.ccTaskLineSevMedium,
									)}
								>
									{g.label}
								</span>
							))}
						</>
					)}
				</div>
			)}

			<div
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.flags,
				)}
			>
				{flags.length === 0 ? (
					company.archived ? (
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.flag,
							)}
						>
							Arkiveret regnskab
						</span>
					) : company.actualBankBalance === null || company.vat === null ? (
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.flag,
							)}
						>
							Overblik kræver flere oplysninger
						</span>
					) : (
						<span
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.flag,
								cockpitStyles.flagOk,
							)}
						>
							Ingen kendte advarsler
						</span>
					)
				) : (
					flags.map((f) =>
						// #420 — flag med et `to`-felt er klikbare: ejeren får et konkret
						// næste skridt (fx Integritet-viewet) i stedet for et skræmmende
						// dødt label.
						f.to ? (
							<Link
								key={f.label}
								to={f.to}
								{...stylex.props(
									cockpitStyles.flagComposition,
									f.level === "critical" && cockpitStyles.flagCritical,
									f.level === "warning" && cockpitStyles.flagWarning,
								)}
							>
								{f.label}
							</Link>
						) : (
							<span
								key={f.label}
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.flag,
									f.level === "critical" && cockpitStyles.flagCritical,
									f.level === "warning" && cockpitStyles.flagWarning,
								)}
							>
								{f.label}
							</span>
						),
					)
				)}
			</div>
		</article>
	);
}
