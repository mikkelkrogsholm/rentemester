import * as stylex from "@stylexjs/stylex";
import { cockpitStyles } from "../design/cockpit.stylex";
// Compatibility adapters for older views. New pages use PageState directly.
import { PageState } from "./CockpitPrimitives";

export function Loading({ label = "Indlæser…" }: { label?: string }) {
	return <PageState kind="loading" title={label} />;
}

export function ErrorState({
	message,
	onRetry,
}: {
	message: string;
	onRetry?: () => void;
}) {
	return (
		<PageState kind="error" title="Siden kunne ikke hentes" onRetry={onRetry}>
			{message}
		</PageState>
	);
}

export function Banner({
	kind,
	children,
}: {
	kind: "error" | "success" | "warning";
	children: React.ReactNode;
}) {
	return (
		<div
			role={kind === "error" ? "alert" : "status"}
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.banner,
				kind === "error" && cockpitStyles.bannerError,
				kind === "success" && cockpitStyles.bannerSuccess,
				kind === "warning" && cockpitStyles.bannerWarning,
			)}
		>
			{children}
		</div>
	);
}
