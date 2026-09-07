import { useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { Banner, ErrorState, Loading } from "../components/Feedback";
import { useAsync } from "../lib/useAsync";
import { workspaceInboxApi } from "../lib/api/workspace-inbox";

/** One deliberately small inbox: ingest → inspect → explicitly assign → handoff. */
export function WorkspaceInboxView() {
  const { slug = "" } = useParams();
  const data = useAsync(() => workspaceInboxApi.list(slug), [slug]);
  const [file, setFile] = useState<File>();
  const [target, setTarget] = useState("");
  const [message, setMessage] = useState<string>();
  const reload = () => data.reload();
  async function ingest(event: FormEvent) {
    event.preventDefault();
    if (!file) {
      setMessage("Vælg en fil.");
      return;
    }
    try {
      const encoded = await new Promise<string>((resolve, reject) => {
        const r = new FileReader();
        r.onerror = () => reject(new Error("Filen kunne ikke læses."));
        r.onload = () => resolve(String(r.result).split(",")[1] ?? "");
        r.readAsDataURL(file);
      });
      await workspaceInboxApi.ingest(slug, {
        idempotencyKey: crypto.randomUUID(),
        bytesBase64: encoded,
        filename: file.name,
        mimeType: file.type || "application/octet-stream",
        transport: "upload",
        receivedAt: new Date().toISOString(),
        metadata: {},
        candidates: [],
      });
      setMessage("Kilden er lagt i workspace-indbakken uden bogføring.");
      reload();
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Indlæsning fejlede.",
      );
    }
  }
  async function act(sourceId: string, action: "assign" | "complete") {
    try {
      if (!target) {
        setMessage("Angiv målvirksomhedens slug efter review.");
        return;
      }
      await workspaceInboxApi[action](slug, sourceId, target);
      setMessage(
        action === "assign"
          ? "Ruting er godkendt. Håndoff er stadig et separat trin."
          : "Kilden er overdraget én gang til virksomhedens dokumentflow.",
      );
      reload();
    } catch (error) {
      setMessage(
        error instanceof Error ? error.message : "Handlingen fejlede.",
      );
    }
  }
  if (data.loading && !data.data)
    return <Loading label="Henter workspace-indbakke…" />;
  if (data.error) return <ErrorState message={data.error} onRetry={reload} />;
  return (
    <section className="page" data-cockpit-page="workspace-inbox" data-evidence-issue="655">
      <header className="page-head">
        <div>
          <p className="eyebrow">Workspace</p>
          <h2>Fælles dokumentindbakke</h2>
          <p className="muted">
            Kilder ligger uden for hovedbogen, indtil en autoriseret bruger har
            valgt virksomhed og fuldført overdragelsen.
          </p>
        </div>
        <button type="button" className="secondary" onClick={reload}>
          Opdater
        </button>
      </header>
      <section className="card">
        <h3>Indlæs kilde</h3>
        <form onSubmit={(event) => void ingest(event)}>
          <input
            aria-label="Inbox-fil"
            type="file"
            onChange={(event) => setFile(event.target.files?.[0])}
          />
          <button className="btn secondary" type="submit" disabled={!file}>
            Indlæs til indbakke
          </button>
        </form>
      </section>
      <section className="card">
        <h3>Afventer review</h3>
        <label>
          Målvirksomhed (slug efter review)
          <input
            aria-label="Inbox målvirksomhed"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
          />
        </label>
        {data.data?.rows.length ? (
          <ul>
            {data.data.rows.map((source) => (
              <li key={source.sourceId}>
                <strong>{source.filename}</strong>{" "}
                <span className="muted">
                  {source.transport} · SHA-256 {source.sha256.slice(0, 12)}… ·{" "}
                  {(source.exception?.code ??
                    source.assignments
                      .map((item) => `${item.companySlug}: ${item.state}`)
                      .join(", ")) ||
                    "kræver review"}
                </span>
                <div className="modal-actions">
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => void act(source.sourceId, "assign")}
                  >
                    Godkend ruting
                  </button>
                  <button
                    type="button"
                    className="secondary"
                    onClick={() => void act(source.sourceId, "complete")}
                  >
                    Fuldfør handoff
                  </button>
                </div>
              </li>
            ))}
          </ul>
        ) : (
          <p className="muted">Ingen synlige kilder.</p>
        )}
      </section>
      {message && (
        <Banner
          kind={
            message.includes("fejl") || message.includes("Angiv")
              ? "error"
              : "success"
          }
        >
          {message}
        </Banner>
      )}
    </section>
  );
}
