import * as stylex from "@stylexjs/stylex";
import { Link } from "react-router-dom";
import { cockpitStyles } from "../design/cockpit.stylex";

/** A canonical-party deep link.  It intentionally renders nothing without an
 * ID: display text is never evidence for resolving a party. */
export function PartyLink({
	slug,
	partyId,
	children,
}: {
	slug: string;
	partyId: string | null | undefined;
	children: React.ReactNode;
}) {
	if (!partyId) return <>{children}</>;
	return (
		<Link
			to={`/companies/${encodeURIComponent(slug)}/parter/${encodeURIComponent(partyId)}${location.search}`}
			{...stylex.props(cockpitStyles.aComposition)}
		>
			{children}
		</Link>
	);
}

export function PartySummary({
	name,
	roles = [],
}: {
	name: string;
	roles?: string[];
}) {
	return (
		<span {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
			{name}
			{roles.length ? (
				<span
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.muted,
					)}
				>
					{" "}
					· {roles.join(", ")}
				</span>
			) : null}
		</span>
	);
}
