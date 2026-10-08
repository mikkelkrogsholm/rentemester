import * as stylex from "@stylexjs/stylex";
import { FormEvent, type ReactNode, useEffect, useState } from "react";
import { Link, useLocation, useNavigate } from "react-router-dom";
import { Button, Input } from "../components/ui";
import { cockpitStyles } from "../design/cockpit.stylex";
import { authClient } from "../lib/auth-client";
import { useAuth } from "../lib/auth-context";

export function LoginView() {
	const { refresh } = useAuth();
	const [email, setEmail] = useState("");
	const [password, setPassword] = useState("");
	const [needsTotp, setNeedsTotp] = useState(false);
	const [verificationMethod, setVerificationMethod] = useState<
		"totp" | "backup"
	>("totp");
	const [code, setCode] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const result = await authClient.signIn.email({
				email,
				password,
				rememberMe: false,
			});
			if (result.error) throw new Error("sign in failed");
			if (
				result.data &&
				"twoFactorRedirect" in result.data &&
				result.data.twoFactorRedirect
			) {
				setPassword("");
				setNeedsTotp(true);
			} else await refresh();
		} catch {
			setError(
				"Kunne ikke logge ind. Kontrollér dine oplysninger og prøv igen.",
			);
		} finally {
			setBusy(false);
		}
	};
	const verify = async (event: FormEvent) => {
		event.preventDefault();
		setBusy(true);
		setError(null);
		try {
			const result =
				verificationMethod === "totp"
					? await authClient.twoFactor.verifyTotp({ code, trustDevice: false })
					: await authClient.twoFactor.verifyBackupCode({
							code,
							trustDevice: false,
						});
			if (result.error) throw new Error("two factor verification failed");
			await refresh();
		} catch {
			setError("Koden kunne ikke bekræftes. Prøv igen.");
		} finally {
			setCode("");
			setBusy(false);
		}
	};
	return (
		<AuthPanel title={needsTotp ? "Bekræft din kode" : "Log ind"}>
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
			{needsTotp ? (
				<form
					onSubmit={verify}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{verificationMethod === "totp"
							? "Indtast koden fra din autentifikator-app."
							: "Indtast én af dine recovery codes."}
					</p>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						{verificationMethod === "totp" ? "Engangskode" : "Recovery code"}
						<Input
							aria-label={
								verificationMethod === "totp" ? "Engangskode" : "Recovery code"
							}
							inputMode={verificationMethod === "totp" ? "numeric" : "text"}
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
						Bekræft
					</Button>
					<Button
						variant="secondary"
						type="button"
						disabled={busy}
						onClick={() => {
							setCode("");
							setError(null);
							setVerificationMethod((current) =>
								current === "totp" ? "backup" : "totp",
							);
						}}
						xstyle={[cockpitStyles.buttonComposition]}
					>
						{verificationMethod === "totp"
							? "Brug recovery code"
							: "Brug autentifikator-kode"}
					</Button>
				</form>
			) : (
				<form
					onSubmit={submit}
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						E-mail
						<Input
							aria-label="E-mail"
							autoComplete="email"
							type="email"
							value={email}
							onChange={(e) => setEmail(e.target.value)}
							required
							xstyle={[cockpitStyles.inputComposition]}
						/>
					</label>
					<label
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Adgangskode
						<Input
							aria-label="Adgangskode"
							autoComplete="current-password"
							type="password"
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
						Log ind
					</Button>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<Link
							to="/forgot-password"
							{...stylex.props(cockpitStyles.aComposition)}
						>
							Glemt adgangskode?
						</Link>
					</p>
					<p
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						<Link
							to="/verify-email"
							{...stylex.props(cockpitStyles.aComposition)}
						>
							Send bekræftelsesmail igen
						</Link>
					</p>
				</form>
			)}
		</AuthPanel>
	);
}

const recoveryMessage =
	"Hvis e-mailadressen kan bruges, modtager du en e-mail med næste trin.";

export function ForgotPasswordView() {
	const [email, setEmail] = useState("");
	const [submitted, setSubmitted] = useState(false);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		try {
			await authClient.requestPasswordReset({
				email,
				redirectTo: `${window.location.origin}/reset-password`,
			});
		} catch {
			/* Deliberately identical response to prevent account enumeration. */
		} finally {
			setEmail("");
			setSubmitted(true);
		}
	};
	return (
		<AuthPanel title="Nulstil adgangskode">
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Indtast din e-mailadresse, så sender vi et reset-link, hvis den kan
				bruges.
			</p>
			{submitted && (
				<p
					role="status"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{recoveryMessage}
				</p>
			)}
			<form
				onSubmit={submit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					E-mail
					<Input
						aria-label="E-mail"
						autoComplete="email"
						type="email"
						value={email}
						onChange={(event) => setEmail(event.target.value)}
						required
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<Button type="submit" xstyle={[cockpitStyles.buttonComposition]}>
					Send reset-link
				</Button>
			</form>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<Link to="/" {...stylex.props(cockpitStyles.aComposition)}>
					Tilbage til login
				</Link>
			</p>
		</AuthPanel>
	);
}

export function VerificationRecoveryView() {
	const [email, setEmail] = useState("");
	const [submitted, setSubmitted] = useState(false);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		try {
			await authClient.sendVerificationEmail({
				email,
				callbackURL: window.location.origin,
			});
		} catch {
			/* Deliberately identical response to prevent account enumeration. */
		} finally {
			setEmail("");
			setSubmitted(true);
		}
	};
	return (
		<AuthPanel title="Bekræft din e-mail">
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				Indtast din e-mailadresse for at få en ny bekræftelsesmail.
			</p>
			{submitted && (
				<p
					role="status"
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					{recoveryMessage}
				</p>
			)}
			<form
				onSubmit={submit}
				{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
			>
				<label
					{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
				>
					E-mail
					<Input
						aria-label="E-mail"
						autoComplete="email"
						type="email"
						value={email}
						onChange={(event) => setEmail(event.target.value)}
						required
						xstyle={[cockpitStyles.inputComposition]}
					/>
				</label>
				<Button type="submit" xstyle={[cockpitStyles.buttonComposition]}>
					Send bekræftelsesmail
				</Button>
			</form>
			<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
				<Link to="/" {...stylex.props(cockpitStyles.aComposition)}>
					Tilbage til login
				</Link>
			</p>
		</AuthPanel>
	);
}

export function ResetPasswordView() {
	const location = useLocation();
	const navigate = useNavigate();
	const [token, setToken] = useState(
		() => new URLSearchParams(location.search).get("token") ?? "",
	);
	const [password, setPassword] = useState("");
	const [error, setError] = useState<string | null>(null);
	const [complete, setComplete] = useState(false);
	useEffect(() => {
		if (location.search) navigate("/reset-password", { replace: true });
	}, [location.search, navigate]);
	const submit = async (event: FormEvent) => {
		event.preventDefault();
		setError(null);
		if (!token) {
			setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link.");
			return;
		}
		try {
			const result = await authClient.resetPassword({
				newPassword: password,
				token,
			});
			if (result.error) throw new Error("reset failed");
			setPassword("");
			setToken("");
			setComplete(true);
		} catch {
			setPassword("");
			setError("Linket er ugyldigt eller udløbet. Anmod om et nyt reset-link.");
		}
	};
	return (
		<AuthPanel title="Vælg ny adgangskode">
			{complete ? (
				<>
					<p
						role="status"
						{...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}
					>
						Din adgangskode er opdateret. Log ind igen.
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
						Vælg en ny adgangskode på mindst 12 tegn.
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
							Ny adgangskode
							<Input
								aria-label="Ny adgangskode"
								autoComplete="new-password"
								type="password"
								minLength={12}
								value={password}
								onChange={(event) => setPassword(event.target.value)}
								required
								xstyle={[cockpitStyles.inputComposition]}
							/>
						</label>
						<Button type="submit" xstyle={[cockpitStyles.buttonComposition]}>
							Opdater adgangskode
						</Button>
					</form>
				</>
			)}
		</AuthPanel>
	);
}

export function AuthPanel({
	title,
	children,
}: {
	title: string;
	children: ReactNode;
}) {
	return (
		<main
			{...stylex.props(
				cockpitStyles.element,
				cockpitStyles.focusVisible,
				cockpitStyles.authPanel,
			)}
		>
			<section
				{...stylex.props(
					cockpitStyles.element,
					cockpitStyles.focusVisible,
					cockpitStyles.card,
				)}
			>
				<p {...stylex.props(cockpitStyles.element, cockpitStyles.focusVisible)}>
					Rentemester
				</p>
				<h1
					{...stylex.props(
						cockpitStyles.element,
						cockpitStyles.focusVisible,
						cockpitStyles.h1,
						cockpitStyles.authPanelH1,
					)}
				>
					{title}
				</h1>
				{children}
			</section>
		</main>
	);
}
