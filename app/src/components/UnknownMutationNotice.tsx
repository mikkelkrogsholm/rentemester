import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { cockpitStyles } from "../design/cockpit.stylex";
import { Button, Dialog } from "./ui";

/** A read never releases a write lock. Only explicit human reconciliation does. */
export function UnknownMutationNotice({
	onRefresh,
	onRelease,
	persistent = true,
	verifyPersistence,
}: {
	onRefresh?: () => unknown | Promise<unknown>;
	onRelease: () => void;
	persistent?: boolean;
	verifyPersistence?: () => void;
}) {
	const [refreshBusy, setRefreshBusy] = useState(false);
	const [readError, setReadError] = useState(false);
	const [confirmRelease, setConfirmRelease] = useState(false);
	async function refresh() {
		setRefreshBusy(true);
		setReadError(false);
		try {
			await onRefresh?.();
		} catch {
			setReadError(true);
		} finally {
			setRefreshBusy(false);
		}
	}
	return (
		<div
			role="alert"
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.banner,
				cockpitStyles.bannerWarning,
			)}
		>
			{persistent ? (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Serverens resultat kunne ikke bekræftes. Handlingen kan være
					gennemført. En ny skrivning er blokeret, også i andre browserfaner og
					efter genindlæsning. Kontrollér resultatet i oversigten eller
					revisionssporet.
				</p>
			) : (
				<>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Skrivehandlinger er blokeret, fordi browseren ikke kan gemme
						beskyttelsen mod gentagelse. Behold denne fane åben, hvis en
						handling blev afbrudt, og kontrollér resultatet før genindlæsning.
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Tillad lokalt browserlager for appen, og kontrollér, at blokeringen
						kan gemmes.
					</p>
					{verifyPersistence && (
						<Button
							variant="secondary"
							onClick={verifyPersistence}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Kontrollér browserens lager
						</Button>
					)}
				</>
			)}
			{onRefresh && (
				<Button
					variant="secondary"
					disabled={refreshBusy}
					onClick={() => void refresh()}
					xstyle={[cockpitStyles.buttonComposition]}
				>
					Kontrollér status
				</Button>
			)}
			<Button
				variant="secondary"
				disabled={refreshBusy || !persistent}
				onClick={() => setConfirmRelease(true)}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				Resultatet er afklaret
			</Button>
			{readError && (
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Status kunne ikke hentes. En ny skrivning er fortsat blokeret.
				</p>
			)}
			{confirmRelease && (
				<Dialog
					title="Frigiv efter kontrol"
					onClose={() => setConfirmRelease(false)}
					xstyle={[cockpitStyles.element, cockpitStyles.focusVisible]}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Frigiv kun formularen, når du har kontrolleret, om handlingen blev
						gennemført. Hvis den blev gennemført, skal du beholde resultatet og
						undgå at gentage den. Frigivelsen sender ingen handling til
						serveren.
					</p>
					<div
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.modalActions,
						)}
					>
						<Button
							variant="secondary"
							onClick={() => setConfirmRelease(false)}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Behold blokering
						</Button>
						<Button
							onClick={() => {
								setConfirmRelease(false);
								onRelease();
							}}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							Jeg har kontrolleret resultatet
						</Button>
					</div>
				</Dialog>
			)}
		</div>
	);
}
