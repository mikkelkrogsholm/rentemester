// MileageRegisterModal — the human "Registrér kørsel" action for the Cockpit
// (#335).
//
// A person opens this from the Kørsel view, fills in the trip's date, purpose,
// from/to, kilometres, vehicle, driver, per-km rate, and the rate-basis note
// (which official rate table the rate came from). The browser POSTs only
// those essentials; the server runs the SAME `createMileageEntry` core path
// the CLI's `mileage add` command uses. The mileage register is append-only
// audit data — the schema's triggers refuse any update or delete — so the
// body carries `confirm: true`.

import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { formatKroner } from "../lib/format";
import type { MileageEntrySummary } from "../lib/types";
import { Banner } from "./Feedback";
import { LockBanner } from "./LockBanner";

/** Shape of the API error the cockpit's `api.ts` throws. */
type MaybeApiError = { code?: string; message?: string };

export type MileageRegisterModalProps = {
  slug: string;
  /** Called after a successful create so the calling view can refresh. */
  onRegistered: () => void;
  /** Closes the modal without acting. */
  onClose: () => void;
};

function todayIsoDate(): string {
  const now = new Date();
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

export function MileageRegisterModal({
  slug,
  onRegistered,
  onClose,
}: MileageRegisterModalProps) {
  const [tripDate, setTripDate] = useState(todayIsoDate());
  const [purpose, setPurpose] = useState("");
  const [fromLocation, setFromLocation] = useState("");
  const [toLocation, setToLocation] = useState("");
  const [kilometers, setKilometers] = useState("");
  const [vehicle, setVehicle] = useState("Privat bil");
  const [driver, setDriver] = useState("");
  const [ratePerKm, setRatePerKm] = useState("");
  // The mileage core deliberately does NOT own a tax rate: `rateBasis` is the
  // free-text, source-backed note the human confirms (e.g. "SKAT
  // befordringsfradrag 2026 (høj sats, op til 20.000 km)"). The link to
  // skat.dk is rendered next to the field as a guidance touchpoint.
  const [rateBasis, setRateBasis] = useState("");
  const [rateSource, setRateSource] = useState("");
  const [notes, setNotes] = useState("");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [locked, setLocked] = useState<string | null>(null);
  const [done, setDone] = useState<MileageEntrySummary | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const firstFieldRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    (firstFieldRef.current ?? closeRef.current)?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !busy) onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [busy, onClose]);

  function reset() {
    // Keep the vehicle + rate-basis fields so repeat trips are quick to log.
    setPurpose("");
    setFromLocation("");
    setToLocation("");
    setKilometers("");
    setNotes("");
    setError(null);
    setLocked(null);
    setDone(null);
  }

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setError(null);
    setLocked(null);

    const km = Number(kilometers);
    const rate = Number(ratePerKm);
    if (!tripDate) return setError("Dato er påkrævet.");
    if (!purpose.trim()) return setError("Formål er påkrævet.");
    if (!fromLocation.trim()) return setError("Fra-adresse er påkrævet.");
    if (!toLocation.trim()) return setError("Til-adresse er påkrævet.");
    if (!Number.isFinite(km) || km <= 0)
      return setError("Antal km skal være et positivt tal.");
    if (!vehicle.trim()) return setError("Køretøj er påkrævet.");
    if (!driver.trim()) return setError("Chauffør er påkrævet.");
    if (!Number.isFinite(rate) || rate <= 0)
      return setError("Takst skal være et positivt tal i kr/km.");
    if (!rateBasis.trim())
      return setError(
        "Takst-grundlag er påkrævet — angiv hvilken officiel sats taksten kommer fra (fx 'SKAT 2026, høj sats').",
      );

    setBusy(true);
    try {
      const result = await api.createMileageEntry(slug, {
        tripDate,
        purpose: purpose.trim(),
        fromLocation: fromLocation.trim(),
        toLocation: toLocation.trim(),
        kilometers: km,
        vehicle: vehicle.trim(),
        driver: driver.trim(),
        ratePerKm: rate,
        rateBasis: rateBasis.trim(),
        ...(rateSource.trim() ? { rateSource: rateSource.trim() } : {}),
        ...(notes.trim() ? { notes: notes.trim() } : {}),
      });
      setDone(result);
      onRegistered();
    } catch (err) {
      const e = err as MaybeApiError;
      // The backup-lock conflict carries a curated Danish message — render it
      // in the dedicated banner so the human gets a calm explanation, not a
      // red error.
      if (e?.code === "conflict" && /[Bb]ogføring er låst/.test(e.message ?? "")) {
        setLocked(e.message!);
      } else {
        setError(e?.message ?? "Kørslen kunne ikke registreres.");
      }
    } finally {
      setBusy(false);
    }
  }

  const amountBasis =
    Number.isFinite(Number(kilometers)) && Number.isFinite(Number(ratePerKm))
      ? Number(kilometers) * Number(ratePerKm)
      : null;

  return (
    <div
      className="modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label="Registrér kørsel"
    >
      <div className="modal">
        <div className="modal-head">
          <h3>Registrér kørsel</h3>
          <button
            type="button"
            ref={closeRef}
            className="btn secondary"
            onClick={onClose}
            disabled={busy}
          >
            Luk
          </button>
        </div>

        {locked && <LockBanner message={locked} />}
        {error && <Banner kind="error">{error}</Banner>}

        {done ? (
          <div className="modal-body">
            <Banner kind="success">
              Kørsel registreret som {done.entryNo} —{" "}
              {done.amountBasis !== null
                ? formatKroner(done.amountBasis, "DKK")
                : "—"}{" "}
              i godtgørelsesgrundlag.
            </Banner>
            <div className="row-actions" style={{ marginTop: "0.75rem" }}>
              <button type="button" className="btn" onClick={reset}>
                Registrér en til
              </button>
              <button type="button" className="btn secondary" onClick={onClose}>
                Færdig
              </button>
            </div>
          </div>
        ) : (
          <form className="modal-body" onSubmit={handleSubmit}>
            <div className="form-grid">
              <label>
                <span>Dato</span>
                <input
                  ref={firstFieldRef}
                  type="date"
                  value={tripDate}
                  onChange={(e) => setTripDate(e.target.value)}
                  required
                />
              </label>
              <label>
                <span>Formål</span>
                <input
                  type="text"
                  value={purpose}
                  onChange={(e) => setPurpose(e.target.value)}
                  placeholder="Kundebesøg Aarhus"
                  required
                />
              </label>
              <label>
                <span>Fra-adresse</span>
                <input
                  type="text"
                  value={fromLocation}
                  onChange={(e) => setFromLocation(e.target.value)}
                  placeholder="København"
                  required
                />
              </label>
              <label>
                <span>Til-adresse</span>
                <input
                  type="text"
                  value={toLocation}
                  onChange={(e) => setToLocation(e.target.value)}
                  placeholder="Aarhus"
                  required
                />
              </label>
              <label>
                <span>Antal km</span>
                <input
                  type="number"
                  step="0.1"
                  min="0"
                  value={kilometers}
                  onChange={(e) => setKilometers(e.target.value)}
                  required
                />
              </label>
              <label>
                <span>Køretøj</span>
                <input
                  type="text"
                  value={vehicle}
                  onChange={(e) => setVehicle(e.target.value)}
                  placeholder="Privat bil"
                  required
                />
              </label>
              <label>
                <span>Chauffør</span>
                <input
                  type="text"
                  value={driver}
                  onChange={(e) => setDriver(e.target.value)}
                  required
                />
              </label>
              <label>
                <span>Takst (kr/km)</span>
                <input
                  type="number"
                  step="0.01"
                  min="0"
                  value={ratePerKm}
                  onChange={(e) => setRatePerKm(e.target.value)}
                  required
                />
              </label>
              <label className="span-2">
                <span>Takst-grundlag</span>
                <input
                  type="text"
                  value={rateBasis}
                  onChange={(e) => setRateBasis(e.target.value)}
                  placeholder="SKAT 2026, høj sats (op til 20.000 km)"
                  required
                />
                <small className="muted">
                  Rentemester ejer ikke skattesatsen — du bekræfter hvilken
                  officiel sats du bruger. Slå op på{" "}
                  <a
                    href="https://skat.dk/erhverv/moms/regler-og-satser/satser-for-erhvervsmaessig-koersel"
                    target="_blank"
                    rel="noreferrer noopener"
                  >
                    skat.dk
                  </a>
                  .
                </small>
              </label>
              <label className="span-2">
                <span>Takst-kilde (valgfri)</span>
                <input
                  type="url"
                  value={rateSource}
                  onChange={(e) => setRateSource(e.target.value)}
                  placeholder="https://skat.dk/…"
                />
              </label>
              <label className="span-2">
                <span>Noter (valgfri)</span>
                <textarea
                  value={notes}
                  onChange={(e) => setNotes(e.target.value)}
                  rows={2}
                />
              </label>
            </div>

            {amountBasis !== null && amountBasis > 0 && (
              <p className="muted" style={{ marginTop: "0.5rem" }}>
                Godtgørelsesgrundlag: {formatKroner(amountBasis, "DKK")} (km ×
                takst). Beløbet er dokumentation — kørselsregisteret bogfører
                aldrig direkte.
              </p>
            )}

            <div className="row-actions" style={{ marginTop: "0.75rem" }}>
              <button type="submit" className="btn" disabled={busy}>
                {busy ? "Registrerer…" : "Registrér kørsel"}
              </button>
              <button
                type="button"
                className="btn secondary"
                onClick={onClose}
                disabled={busy}
              >
                Annullér
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
