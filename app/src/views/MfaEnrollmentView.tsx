import * as stylex from "@stylexjs/stylex";
import { FormEvent, useState } from "react";
import { Button, Input } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";
import { AuthPanel } from "./LoginView";

export function VerificationRequiredView() {
	const { session } = useAuth();
	const [sent, setSent] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const resend = async () => {
		setError(null);
		try {
			const result = await authClient.sendVerificationEmail({
				email: session?.email ?? "",
				callbackURL: "/",
			});
			if (result.error) throw new Error("resend failed");
			setSent(true);
		} catch {
			setError("Kunne ikke sende bekræftelsesmailen. Prøv igen senere.");
		}
	};
	return (
		<AuthPanel title="Bekræft din e-mail">
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Du skal bekræfte din e-mail, før du kan fortsætte.
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
			<Button
				type="button"
				onClick={() => void resend()}
				xstyle={[cockpitStyles.buttonComposition]}
			>
				{sent ? "E-mail sendt" : "Send bekræftelsesmail igen"}
			</Button>
		</AuthPanel>
	);
}

export function MfaEnrollmentView() {
	const { refresh } = useAuth();
	const [password, setPassword] = useState("");
	const [uri, setUri] = useState<string | null>(null);
	const [codes, setCodes] = useState<string[]>([]);
	const [code, setCode] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const begin = async (event: FormEvent) => {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const result = await authClient.twoFactor.enable({
				password,
				method: "totp",
				issuer: "Rentemester",
			});
			if (result.error) throw new Error("totp enable failed");
			if (!result.data || result.data.method !== "totp")
				throw new Error("TOTP setup was not returned");
			setUri(result.data.totpURI);
			setCodes(result.data.backupCodes);
			setPassword("");
		} catch {
			setError("Kunne ikke starte opsætningen. Prøv igen.");
		} finally {
			setBusy(false);
		}
	};
	const verify = async (event: FormEvent) => {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const result = await authClient.twoFactor.verifyTotp({
				code,
				trustDevice: false,
			});
			if (result.error) throw new Error("totp verify failed");
			setCodes([]);
			await refresh();
		} catch {
			setError("Koden kunne ikke bekræftes. Prøv igen.");
		} finally {
			setBusy(false);
		}
	};
	return (
		<AuthPanel title="Opsæt totrinsbekræftelse">
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
			{!uri ? (
				<form
					onSubmit={begin}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Tilføj Rentemester i din autentifikator-app.
					</p>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Adgangskode
						<Input
							aria-label="Adgangskode"
							type="password"
							autoComplete="current-password"
							value={password}
							onChange={(e) => setPassword(e.target.value)}
							required
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
					<Button
						type="submit"
						disabled={busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Start opsætning
					</Button>
				</form>
			) : (
				<form
					onSubmit={verify}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Indtast denne opsætningsadresse i din autentifikator-app:
					</p>
					<code
						{...stylex.props(
							cockpitStyles.element,
							cockpitStyles.focusVisible,
							cockpitStyles.code,
							cockpitStyles.totpUri,
						)}
					>
						{uri}
					</code>
					{codes.length > 0 && (
						<section
							aria-label="Recovery codes"
							{...stylex.props(
								cockpitStyles.element,
								cockpitStyles.focusVisible,
								cockpitStyles.recoveryCodes,
							)}
						>
							<h3
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.h3,
									cockpitStyles.recoveryCodesH3,
								)}
							>
								Recovery codes
							</h3>
							<p
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
								)}
							>
								Gem dem nu. De vises ikke igen.
							</p>
							<ul
								{...stylex.props(
									cockpitStyles.element,
									cockpitStyles.focusVisible,
									cockpitStyles.recoveryCodesUl,
								)}
							>
								{codes.map((backup) => (
									<li
										key={backup}
										{...stylex.props(
											cockpitStyles.element,
											cockpitStyles.focusVisible,
										)}
									>
										{backup}
									</li>
								))}
							</ul>
						</section>
					)}
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Engangskode
						<Input
							aria-label="Engangskode"
							inputMode="numeric"
							autoComplete="one-time-code"
							value={code}
							onChange={(e) => setCode(e.target.value)}
							required
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
					<Button
						type="submit"
						disabled={busy}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						Bekræft opsætning
					</Button>
				</form>
			)}
		</AuthPanel>
	);
}
