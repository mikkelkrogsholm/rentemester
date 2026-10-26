import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { useEffect, useRef, useState, type RefObject } from "react";
import { authClient } from "../lib/auth-client";
import { Button, Dialog, Field, Input } from "./ui";
import { ConfirmDialog } from "./ConfirmDialog";

type Operation = "view" | "generate";
const unknownRegeneration =
  "Kunne ikke bekræfte resultatet. Tidligere koder kan være erstattet. Indtast adgangskoden igen og vælg Vis resterende koder for at kontrollere.";

export function BackupCodesDialog({
  onClose,
  returnFocusRef,
}: {
  onClose: () => void;
  returnFocusRef: RefObject<HTMLButtonElement>;
}) {
  const [password, setPassword] = useState("");
  const [codes, setCodes] = useState<string[] | null>(null);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const generation = useRef(0);
  const activeOperation = useRef<Operation | null>(null);
  useEffect(() => {
    const hide = () => {
      generation.current++;
      setPassword("");
      setCodes(null);
      setConfirm(false);
      setError(
        activeOperation.current === "generate"
          ? unknownRegeneration
          : activeOperation.current === "view"
            ? "Visningen blev skjult. Indtast adgangskoden igen for at hente koderne."
            : null,
      );
    };
    const visibility = () => {
      if (document.visibilityState === "hidden") hide();
    };
    document.addEventListener("visibilitychange", visibility);
    window.addEventListener("pagehide", hide);
    return () => {
      generation.current++;
      document.removeEventListener("visibilitychange", visibility);
      window.removeEventListener("pagehide", hide);
    };
  }, []);
  const request = async (operation: Operation) => {
    if (!password || busy) return;
    const current = ++generation.current;
    activeOperation.current = operation;
    setBusy(true);
    setCodes(null);
    setError(null);
    setConfirm(false);
    const entered = password;
    setPassword("");
    try {
      const result = await authClient.$fetch<{
        status: boolean;
        backupCodes: unknown;
      }>(
        `/two-factor/${operation === "view" ? "view" : "generate"}-backup-codes`,
        {
          method: "POST",
          body: { password: entered },
          retry: 0,
        },
      );
      if (
        generation.current !== current ||
        document.visibilityState === "hidden"
      )
        return;
      if (result.error?.status === 404) {
        setError("Sikkerhedskoder er ikke tilgængelige på denne server.");
        return;
      }
      const values = result.data?.backupCodes;
      if (
        result.error ||
        result.data?.status !== true ||
        !Array.isArray(values) ||
        values.length > 10 ||
        (operation === "generate" && values.length !== 10) ||
        !values.every(
          (v): v is string =>
            typeof v === "string" && /^[A-Za-z0-9]{5}-[A-Za-z0-9]{5}$/.test(v),
        ) ||
        new Set(values).size !== values.length
      )
        throw new Error("unconfirmed backup operation");
      setCodes(values);
    } catch {
      if (generation.current === current)
        setError(
          operation === "generate"
            ? unknownRegeneration
            : "Koderne kunne ikke hentes. Log ind igen, og prøv med din adgangskode.",
        );
    } finally {
      activeOperation.current = null;
      setBusy(false);
    }
  };
  return (
    <Dialog
      title="Sikkerhedskoder"
      onClose={onClose}
      busy={busy}
      returnFocusRef={returnFocusRef}
    >
      <div {...stylex.props(...[cockpitStyles.card])}>
        <p>
          Hver kode kan bruges én gang i stedet for en kode fra din
          autentifikator-app. Opbevar dem et sikkert sted.
        </p>
        {error && (
          <p role="alert" {...stylex.props(cockpitStyles.banner)}>
            {error}
          </p>
        )}
        {codes !== null ? (
          <>
            <p role="status">
              {codes.length
                ? "Gem koderne sikkert. De skjules, når du lukker eller forlader visningen."
                : "Ingen sikkerhedskoder tilbage. Opret nye koder, før du får brug for dem."}
            </p>
            <ul aria-label="Resterende sikkerhedskoder">
              {codes.map((code) => (
                <li key={code}>
                  <code>{code}</code>
                </li>
              ))}
            </ul>
            <Button variant="secondary" onClick={() => setCodes(null)}>
              Skjul koder
            </Button>
          </>
        ) : (
          <form
            {...stylex.props(cockpitStyles.workflowForm)}
            onSubmit={(event) => {
              event.preventDefault();
              void request("view");
            }}
          >
            <Field
              label="Adgangskode til sikkerhedskoder"
              help="Du skal bekræfte din adgangskode for at se eller erstatte koderne."
            >
              <Input
                type="password"
                autoComplete="current-password"
                required
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                disabled={busy}
              />
            </Field>
            <div {...stylex.props(cockpitStyles.modalActions)}>
              <Button type="submit" busy={busy} disabled={!password}>
                Vis resterende koder
              </Button>
              <Button
                variant="danger"
                type="button"
                disabled={busy || !password}
                onClick={() => setConfirm(true)}
              >
                Opret nye koder
              </Button>
            </div>
          </form>
        )}
        <div {...stylex.props(cockpitStyles.modalActions)}>
          <Button variant="secondary" disabled={busy} onClick={onClose}>
            Luk og skjul
          </Button>
        </div>
      </div>
      {confirm && (
        <ConfirmDialog
          title="Erstat sikkerhedskoder?"
          body={
            <p>
              Alle tidligere sikkerhedskoder stopper med at virke. Gem de nye
              koder, når de vises.
            </p>
          }
          confirmLabel="Erstat koder"
          confirmKind="danger"
          onClose={() => setConfirm(false)}
          onConfirm={() => request("generate")}
        />
      )}
    </Dialog>
  );
}
