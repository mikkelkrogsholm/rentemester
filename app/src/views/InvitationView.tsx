import * as stylex from "@stylexjs/stylex";
import { FormEvent, useEffect, useState } from "react";
import { Link, useLocation } from "react-router-dom";
import { Button, Input } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { api } from "../lib/api";
import { AuthPanel } from "./LoginView";

export function InvitationView() {
	const location = useLocation();
	const [token, setToken] = useState(
		() => new URLSearchParams(window.location.hash.slice(1)).get("token") ?? "",
	);
	const [name, setName] = useState("");
	const [password, setPassword] = useState("");
	const [busy, setBusy] = useState(false);
	const [complete, setComplete] = useState(false);
	const [error, setError] = useState<string | null>(null);

	useEffect(() => {
		if (window.location.hash)
			window.history.replaceState(null, "", location.pathname);
	}, [location.pathname]);

	async function submit(event: FormEvent) {
		event.preventDefault();
		setError(null);
		if (!token) {
			setError("Invitationen er ugyldig eller udløbet.");
			return;
		}
		setBusy(true);
		try {
			await api.claimWorkspaceInvitation({ token, name, password });
			setToken("");
			setPassword("");
			setComplete(true);
		} catch {
			setPassword("");
			setError("Invitationen er ugyldig, udløbet eller allerede brugt.");
		} finally {
			setBusy(false);
		}
	}

	return (
		<AuthPanel title="Acceptér invitation">
			{complete ? (
				<>
					<p
						role="status"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Invitationen er accepteret.
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Bekræft din e-mail, log ind og opsæt din autentifikator, før
						virksomheden kan åbnes.
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<Link to="/" {...stylex.props(cockpitStyles.aComposition)}>
							Til login
						</Link>
					</p>
				</>
			) : (
				<>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Opret din bruger. Virksomhedsdata åbnes først efter
						e-mailbekræftelse og MFA.
					</p>
					{error && (
						<p
							role="alert"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.banner,
								cockpitStyles.bannerError,
							)}
						>
							{error}
						</p>
					)}
					<form
						onSubmit={submit}
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Navn
							<Input
								aria-label="Navn"
								autoComplete="name"
								value={name}
								onChange={(event) => setName(event.target.value)}
								required
								xstyle={[cockpitStyles.inputComposition]}
							/>
						</label>
						<label
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
							)}
						>
							Adgangskode
							<Input
								aria-label="Adgangskode"
								autoComplete="new-password"
								type="password"
								minLength={12}
								value={password}
								onChange={(event) => setPassword(event.target.value)}
								required
								xstyle={[cockpitStyles.inputComposition]}
							/>
						</label>
						<Button
							type="submit"
							disabled={busy}
							xstyle={[cockpitStyles.buttonComposition]}
						>
							{busy ? "Accepterer…" : "Acceptér invitation"}
						</Button>
					</form>
				</>
			)}
		</AuthPanel>
	);
}
