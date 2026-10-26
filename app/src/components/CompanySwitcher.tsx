import * as stylex from "@stylexjs/stylex";
import { useLocation, useNavigate } from "react-router-dom";
import { cockpitStyles } from "../design/cockpit.stylex";
import { useAuth } from "../lib/auth-context";
import { Select } from "./ui";

const roleLabels: Record<string, string> = {
	owner: "Ejer",
	bookkeeper: "Bogholder",
	reviewer: "Godkender",
	reader: "Læseadgang",
};

export function CompanySwitcher() {
	const { context } = useAuth();
	const navigate = useNavigate();
	const location = useLocation();
	if (!context || context.companies.length === 0) return null;
	const current = /^\/companies\/([^/]+)/.exec(location.pathname)?.[1] ?? "";
	return (
		<label
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.companySwitcher,
			)}
		>
			<span
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				Virksomhed
			</span>
			<Select
				aria-label="Skift virksomhed"
				value={current}
				onChange={(event) => navigate(`/companies/${event.target.value}`)}
				xstyle={[cockpitStyles.companySwitcherSelectComposition]}
			>
				<option
					value=""
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					Vælg virksomhed
				</option>
				{context.companies.map((company) => (
					<option
						key={company.slug}
						value={company.slug}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{company.name} — {roleLabels[company.role] ?? "Rolle ukendt"}
					</option>
				))}
			</Select>
		</label>
	);
}
