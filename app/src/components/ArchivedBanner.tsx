// ArchivedBanner — the "Arkiveret regnskabsår — skrivebeskyttet" notice shown
// above an archived year's data in the core views (Runde 3, iteration 10).
//
// The rich views (Resultatopgørelse, Balance, Saldobalance, Posteringer,
// Overblik) now render the read-only #197 archive data for a pre-cut-over
// year; this banner makes it unmistakable that the figures come from a prior
// system's export and cannot be edited. Styled with the shared `archive-banner`
// class already used by the Arkiv view.

export function ArchivedBanner({
  year,
  source,
}: {
  year: string;
  /** The archive's source system, e.g. "dinero"; null when unknown. */
  source: string | null;
}) {
  return (
    <div className="card archive-banner">
      <span className="flag warning">Arkiveret</span>
      <p>
        <strong>Arkiveret regnskabsår {year} — skrivebeskyttet.</strong>{" "}
        Tallene er importeret fra{" "}
        {source
          ? `virksomhedens tidligere ${source}-regnskab`
          : "et tidligere regnskab"}{" "}
        og ligger uden for den aktive bogføring. De kan ikke redigeres.
      </p>
    </div>
  );
}
