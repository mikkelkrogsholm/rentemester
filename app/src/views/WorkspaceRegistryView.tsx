import { useState, type FormEvent } from "react";
import { useParams } from "react-router-dom";
import { useAsync } from "../lib/useAsync";
import { workspaceRegistryApi } from "../lib/api/workspace-registry";
import { Banner, ErrorState, Loading } from "../components/Feedback";

type Action = {
  label: string;
  help: string;
  initial: string;
  run: (body: Record<string, unknown>) => Promise<unknown>;
};
const parse = (value: string): Record<string, unknown> => {
  const result = JSON.parse(value);
  if (!result || Array.isArray(result) || typeof result !== "object")
    throw new Error("Indtast et JSON-objekt.");
  return result as Record<string, unknown>;
};

/** Thin Cockpit adapter: the server remains the single role/business gate. */
function ActionForm({
  action,
  onDone,
}: {
  action: Action;
  onDone: () => void;
}) {
  const [payload, setPayload] = useState(action.initial);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string>();
  async function submit(event: FormEvent) {
    event.preventDefault();
    setResult(undefined);
    try {
      setBusy(true);
      await action.run(parse(payload));
      setResult("Handlingen er registreret i revisionssporet.");
      onDone();
    } catch (error) {
      setResult(
        error instanceof Error
          ? error.message
          : "Handlingen kunne ikke udføres.",
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <form onSubmit={(event) => void submit(event)} className="modal-body">
      <p className="muted">
        {action.help} Serveren kontrollerer din aktuelle rolle og adgang.
      </p>
      <label className="modal-field">
        JSON-data
        <textarea
          aria-label={`${action.label} JSON`}
          value={payload}
          onChange={(event) => setPayload(event.target.value)}
          rows={5}
        />
      </label>
      <label className="modal-checkbox">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(event) => setConfirmed(event.target.checked)}
        />{" "}
        Jeg bekræfter denne auditerede ændring
      </label>
      <div className="modal-actions">
        <button
          className="btn secondary"
          type="submit"
          disabled={!confirmed || busy}
        >
          {busy ? "Arbejder…" : action.label}
        </button>
      </div>
      {result && (
        <Banner kind={result.includes("registreret") ? "success" : "error"}>
          {result}
        </Banner>
      )}
    </form>
  );
}

function RecordIngest({ slug, onDone }: { slug: string; onDone: () => void }) {
  const [file, setFile] = useState<File>();
  const [type, setType] = useState("other");
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string>();
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!file) {
      setResult("Vælg en fil først.");
      return;
    }
    if (file.size > 8 * 1024 * 1024) {
      setResult(
        "Filen er for stor til Cockpit; brug CLI/MCP for den kontrollerede import.",
      );
      return;
    }
    try {
      setBusy(true);
      const bytesBase64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onerror = () => reject(new Error("Filen kunne ikke læses."));
        reader.onload = () =>
          resolve(String(reader.result).split(",")[1] ?? "");
        reader.readAsDataURL(file);
      });
      await workspaceRegistryApi.recordIngest(slug, {
        type,
        bytesBase64,
        filename: file.name,
        source: "cockpit_upload",
        receivedAt: new Date().toISOString(),
        uploader: "cockpit",
        sensitivity: "normal",
        links: [{ type: "company", id: slug }],
      });
      setResult("Den immutable record er indlæst.");
      onDone();
    } catch (error) {
      setResult(
        error instanceof Error ? error.message : "Record kunne ikke indlæses.",
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <form onSubmit={(event) => void submit(event)} className="modal-body">
      <p className="muted">
        Filen gemmes som immutable original med SHA-256. Upload aldrig
        credentials eller hemmeligheder.
      </p>
      <label className="modal-field">
        Fil
        <input
          aria-label="Corporate record fil"
          type="file"
          onChange={(event) => setFile(event.target.files?.[0])}
        />
      </label>
      <label className="modal-field">
        Type
        <select value={type} onChange={(event) => setType(event.target.value)}>
          <option value="other">Andet governance-materiale</option>
          <option value="articles">Vedtægter</option>
          <option value="registration">Registrering</option>
          <option value="board_resolution">Bestyrelsesbeslutning</option>
          <option value="ownership_register">Ejerbog</option>
        </select>
      </label>
      <label className="modal-checkbox">
        <input
          type="checkbox"
          checked={confirmed}
          onChange={(event) => setConfirmed(event.target.checked)}
        />{" "}
        Jeg bekræfter upload af dette governance-dokument
      </label>
      <div className="modal-actions">
        <button
          className="btn secondary"
          type="submit"
          disabled={!file || !confirmed || busy}
        >
          {busy ? "Indlæser…" : "Indlæs immutable record"}
        </button>
      </div>
      {result && (
        <Banner kind={result.includes("indlæst") ? "success" : "error"}>
          {result}
        </Banner>
      )}
    </form>
  );
}

export function WorkspaceRegistryView() {
  const { slug = "" } = useParams();
  const [asOf, setAsOf] = useState(() => new Date().toISOString().slice(0, 10));
  const parties = useAsync(
    () => workspaceRegistryApi.workspaceParties(slug),
    [slug],
  );
  const records = useAsync(
    () => workspaceRegistryApi.corporateRecords(slug),
    [slug],
  );
  const knowledge = useAsync(
    () => workspaceRegistryApi.companyKnowledge(slug),
    [slug],
  );
  const ownership = useAsync(
    () => workspaceRegistryApi.ownership(slug, asOf),
    [slug, asOf],
  );
  const history = useAsync(
    () => workspaceRegistryApi.ownershipHistory(slug),
    [slug],
  );
  const reload = () => {
    parties.reload();
    records.reload();
    knowledge.reload();
    ownership.reload();
    history.reload();
  };
  if (
    (parties.loading || records.loading || knowledge.loading) &&
    !parties.data &&
    !records.data
  )
    return <Loading label="Henter workspace-register…" />;
  if (parties.error || records.error || knowledge.error)
    return (
      <ErrorState
        message={
          parties.error ??
          records.error ??
          knowledge.error ??
          "Register unavailable"
        }
        onRetry={reload}
      />
    );
  const context = knowledge.data?.context;
  const partyActions: Action[] = [
    {
      label: "Opret part",
      run: (body) => workspaceRegistryApi.partyCreate(slug, body),
      initial:
        '{"name":"","kind":"organization","role":"vendor","source":"manual","observedAt":"2026-01-01","reviewAssertion":""}',
      help: "Opretter en canonical part og én lokal virksomhedsrolle.",
    },
    {
      label: "Knyt rolle",
      run: (body) =>
        workspaceRegistryApi.partyRole(slug, String(body.partyId ?? ""), body),
      initial: '{"partyId":"party-id","role":"vendor"}',
      help: "Knytter en rolle kun til denne virksomhed.",
    },
    {
      label: "Foreslå merge",
      run: (body) => workspaceRegistryApi.partyMerge(slug, "propose", body),
      initial: '{"fromPartyId":"","intoPartyId":"","reviewAssertion":""}',
      help: "Foreslår en reviewet merge; den udføres aldrig automatisk.",
    },
    {
      label: "Godkend supersession",
      run: (body) => workspaceRegistryApi.partyMerge(slug, "approve", body),
      initial: '{"fromPartyId":"","proposalHash":""}',
      help: "Godkender præcis den reviewede proposal-hash append-only.",
    },
  ];
  const recordActions: Action[] = [
    {
      label: "Knyt record",
      run: (body) =>
        workspaceRegistryApi.recordAction(
          slug,
          String(body.recordId ?? ""),
          "link",
          body,
        ),
      initial: `{"recordId":"","type":"company","id":"${slug}"}`,
      help: "Knytter eksisterende immutable evidence med en typed reference.",
    },
    {
      label: "Berig metadata",
      run: (body) =>
        workspaceRegistryApi.recordAction(
          slug,
          String(body.recordId ?? ""),
          "enrich",
          body,
        ),
      initial: '{"recordId":"","assertion":""}',
      help: "Tilføjer provenance uden at ændre filens bytes eller hash.",
    },
    {
      label: "Supersedér record",
      run: (body) =>
        workspaceRegistryApi.recordAction(
          slug,
          String(body.recordId ?? ""),
          "supersede",
          body,
        ),
      initial: '{"recordId":"","replacementRecordId":"","reason":""}',
      help: "Opretter en append-only korrektionskæde; ingen original overskrives.",
    },
  ];
  const knowledgeActions: Action[] = [
    {
      label: "Foreslå viden",
      run: (body) =>
        workspaceRegistryApi.knowledgeMutate(slug, "propose", body),
      initial:
        '{"predicate":"business_description","value":"","source":{"kind":"user","ref":""},"validFrom":"2026-01-01","certainty":"confirmed"}',
      help: "Foreslår én kildeunderbygget, effektivt dateret assertion.",
    },
    {
      label: "Review assertion",
      run: (body) => workspaceRegistryApi.knowledgeMutate(slug, "review", body),
      initial: '{"assertionId":"","decision":"approved","reason":""}',
      help: "Godkender eller afviser én eksisterende assertion.",
    },
    {
      label: "Supersedér assertion",
      run: (body) =>
        workspaceRegistryApi.knowledgeMutate(slug, "supersede", body),
      initial:
        '{"assertionId":"","replacement":{"predicate":"business_description","value":"","source":{"kind":"user","ref":""},"validFrom":"2026-01-01"}}',
      help: "Erstatter kun godkendt viden via en ny, reviewbar assertion.",
    },
  ];
  const ownershipActions: Action[] = [
    {
      label: "Foreslå snapshot",
      run: (body) =>
        workspaceRegistryApi.ownershipMutate(slug, "propose", body),
      initial:
        '{"source":"registry","observedAt":"2026-01-01T00:00:00.000Z","facts":[]}',
      help: "Gemmer en kilde-hashet legal observation og deterministic diff.",
    },
    {
      label: "Review snapshot",
      run: (body) => workspaceRegistryApi.ownershipMutate(slug, "review", body),
      initial: '{"snapshotId":"","decision":"approved"}',
      help: "Godkender eller afviser den konkrete snapshot uden at ændre facts.",
    },
    {
      label: "Apply eksakt diff",
      run: (body) => workspaceRegistryApi.ownershipMutate(slug, "apply", body),
      initial: '{"snapshotId":"","snapshotHash":"","diffHash":""}',
      help: "Anvender kun den eksakt reviewede hash; adgang til alle endpoints genkontrolleres.",
    },
  ];
  return (
    <section className="page" data-cockpit-page="workspace-register" data-evidence-issue="655">
      <header className="page-head">
        <div>
          <p className="eyebrow">Workspace</p>
          <h2>Parter og governance-records</h2>
          <p className="muted">
            Kun relationer, der er synlige i denne virksomhed, vises. Originale
            records ændres aldrig her.
          </p>
        </div>
        <button type="button" className="secondary" onClick={reload}>
          Opdater
        </button>
      </header>
      <div className="split-grid">
        <section className="card">
          <h3>Canonical parter</h3>
          {parties.data?.rows.length ? (
            <ul>
              {parties.data.rows.map((p) => (
                <li id={`party-${p.partyId}`} key={p.partyId}>
                  <strong>{p.name}</strong>{" "}
                  <span className="muted">
                    {p.kind} · {p.roles.map((r) => r.role).join(", ")}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">Ingen synlige parter endnu.</p>
          )}
          <details>
            <summary>Administrér parter</summary>
            {partyActions.map((action) => (
              <ActionForm key={action.label} action={action} onDone={reload} />
            ))}
          </details>
        </section>
        <section className="card">
          <h3>Corporate records</h3>
          {records.data?.rows.length ? (
            <ul>
              {records.data.rows.map((r) => (
                <li key={r.recordId}>
                  <strong>{r.filename}</strong>{" "}
                  <span className="muted">
                    {r.type} · SHA-256 {r.sha256.slice(0, 12)}…
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">Ingen synlige governance-records endnu.</p>
          )}
          <details>
            <summary>Indlæs og vedligehold records</summary>
            <RecordIngest slug={slug} onDone={reload} />
            {recordActions.map((action) => (
              <ActionForm key={action.label} action={action} onDone={reload} />
            ))}
          </details>
        </section>
        <section className="card">
          <h3>Virksomhedskontekst</h3>
          {context?.conflicts.length ? (
            <p className="error">
              Konflikt kræver review: {context.conflicts.join(", ")}
            </p>
          ) : null}
          {context?.assertions.length ? (
            <ul>
              {context.assertions.map((a) => (
                <li key={a.assertionId}>
                  <strong>{a.predicate}</strong>{" "}
                  <span className="muted">
                    {a.reviewState} · {a.source.kind}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">
              Ingen godkendt, kildeunderbygget kontekst endnu.
            </p>
          )}
          <details>
            <summary>Foreslå, review og supersedér</summary>
            {knowledgeActions.map((action) => (
              <ActionForm key={action.label} action={action} onDone={reload} />
            ))}
          </details>
        </section>
        <section className="card">
          <h3>Ejer- og kontrolforhold</h3>
          <label>
            Pr. dato{" "}
            <input
              aria-label="Ownership as-of"
              type="date"
              value={asOf}
              onChange={(e) => setAsOf(e.target.value)}
            />
          </label>
          <p className="muted">
            {ownership.data?.partial
              ? "Delvist synlige relationer; skjulte endpoints og antal vises ikke."
              : (ownership.data?.consolidation?.reason ??
                "Ownership-forhold kan ikke vises med den aktuelle adgang.")}
          </p>
          {ownership.data?.facts?.length ? (
            <ul>
              {ownership.data.facts.map((fact, index) => (
                <li key={index}>
                  <code>
                    {fact.owner.kind === "company"
                      ? fact.owner.companySlug
                      : fact.owner.partyId}
                  </code>{" "}
                  → <code>{fact.ownedCompanySlug}</code> ·{" "}
                  {fact.economicBasisPoints != null
                    ? `${fact.economicBasisPoints / 100}%`
                    : "interval"}{" "}
                  · {fact.controlType}
                </li>
              ))}
            </ul>
          ) : (
            <p className="muted">Ingen synlige, godkendte facts.</p>
          )}
          <details>
            <summary>Forslag, review og apply</summary>
            <p className="muted">
              Historik er append-only. Uunderstøttede projektioner vises aldrig
              som konsolidering; brug kun den eksplicitte serverstatus.
            </p>
            {ownershipActions.map((action) => (
              <ActionForm key={action.label} action={action} onDone={reload} />
            ))}
            <ul>
              {history.data?.history?.map((item) => (
                <li key={item.snapshotId}>
                  <code>{item.snapshotId}</code> · {item.state} ·{" "}
                  <code>{item.snapshotHash.slice(0, 12)}…</code>
                </li>
              ))}
            </ul>
          </details>
        </section>
      </div>
    </section>
  );
}
