import { useMutationOutcome } from "../lib/useMutationOutcome";
import { useUnsavedChanges } from "../lib/useUnsavedChanges";
import { ButtonLink, Button, Input, PageHeader } from "../components/ui";
// Bilagsmail view (#348/#350/#351). Tre paneler:
//   1. Mail-alias — virksomhedens unikke localpart (#350).
//   2. IMAP-config — host/port/username/password (#348). Skrives til
//      <companyRoot>/config/imap.json (uden for ledger-DB'en).
//   3. Inbox — senest indlæste mail-drop-dokumenter med status (#351).
//
// IMAP-polling i serve-daemon (#349) styres af `--imap-poll-interval-sec` på
// kommandolinjen — den parameter er ikke synlig her, men status-feltet på
// tabellen viser hvilke dokumenter der allerede er indlæst.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { api, ApiError } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { formatKroner } from "../lib/format";
import type { CompanyBilagsmail } from "../lib/types";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { ErrorState, Loading } from "../components/Feedback";

export function BilagsmailView() {
  const { slug = "" } = useParams();
  return <CompanyMail key={slug} slug={slug} />;
}
function CompanyMail({slug}:{slug:string}) {
  const [error, setError] = useState<string | null>(null);
  const state = useAsync<CompanyBilagsmail>(
    (signal) => api.bilagsmail(slug, { signal }),
    [slug],
  );

  const doneRefresh = state.reload;

  if (state.loading && !state.data) return <Loading />;
  if (state.error && !state.data) return <ErrorState message={state.error} onRetry={state.reload} />;
  const data = state.data!;
  const currency = data.company.currency || "DKK";

  return (
    <section className="bilagsmail-view">
      {state.error && <div className="banner warning" role="alert">Status kunne ikke opdateres. Din formular er bevaret; oplysningerne bag den er fra den seneste gennemførte læsning.</div>}
      <PageHeader title="Bilagsmail" actions={<><div className="row-actions">
          <ButtonLink className="btn secondary" to={`/companies/${slug}/manage`}>
            Administrér
          </ButtonLink>
        </div></>}>
        <div>

          <p className="muted">
            {data.company.cvr ? `CVR ${data.company.cvr} · ` : ""}
            {data.company.country} · Bilagsmail
          </p>
        </div>

      </PageHeader>

      {error && (
        <div className="callout danger" role="alert">
          {error}
        </div>
      )}

      <AliasPanel
        slug={slug}
        initial={data.mailAlias}
        onDone={doneRefresh}
        onError={setError}
      />

      <ImapConfigPanel
        slug={slug}
        configured={data.imapConfigured}
        status={data.imapStatus}
        onDone={doneRefresh}
        onError={setError}
      />

      <PollingPanel slug={slug} data={data} onDone={doneRefresh} />
      <InboxPanel inbox={data.inbox} currency={currency} />
    </section>
  );
}

function AliasPanel({
  slug,
  initial,
  onDone,
  onError,
}: {
  slug: string;
  initial: string | null;
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [alias, setAlias] = useState(initial ?? "");
  const [saving, setSaving] = useState(false);
  const outcome = useMutationOutcome(onDone);
  useUnsavedChanges(alias !== (initial ?? ""));

  const save = async (e: React.FormEvent) => {e.preventDefault(); if (outcome.isBlocked()) return;
    setSaving(true);
    try {
      await api.setBilagsmailAlias(slug, alias.trim() ? alias.trim() : null).catch(outcome.reject);
      onDone();
    } catch (err) {
      onError(err instanceof ApiError ? err.message : "Kunne ikke gemme alias.");
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="card">
    {outcome.feedback}
      <h3>Mail-alias</h3>
      <p className="muted">
        Virksomhedens unikke localpart i bilagsmail-adressen
        (<code>&lt;alias&gt;@bilag.din-host.tld</code>). 3-64 tegn, små bogstaver,
        cifre, punkt, underscore, bindestreg. Tom = ryd alias'et. Et alias opretter ikke et domæne eller en videresendelse; din mailudbyder skal være sat op til at levere til den konfigurerede postkasse.
      </p>
      <form onSubmit={save} className="filter-bar">
        <label>
          Alias
          <Input disabled={outcome.blocked}
            type="text"
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder="fx 'acme-aps'"
            maxLength={64}
          />
        </label>
        <Button requiredPermission="company.admin" type="submit" className="btn primary" disabled={outcome.blocked || (saving)}>
          {saving ? "Gemmer …" : "Gem alias"}
        </Button>
      </form>
    </section>
  );
}

function ImapConfigPanel({
  slug,
  configured,
  status,
  onDone,
  onError,
}: {
  slug: string;
  configured: boolean;
  status: CompanyBilagsmail["imapStatus"];
  onDone: () => void;
  onError: (msg: string) => void;
}) {
  const [host, setHost] = useState(status?.host ?? "");
  const [port, setPort] = useState(String(status?.port ?? 993));
  const [username, setUsername] = useState(status?.username ?? "");
  const [password, setPassword] = useState("");
  const [secure, setSecure] = useState(status?.secure ?? true);
  const [mailbox, setMailbox] = useState(status?.mailbox ?? "INBOX");
  const [saving, setSaving] = useState(false);
  const [pendingDelete, setPendingDelete] = useState(false);
  const outcome = useMutationOutcome(onDone);
  useUnsavedChanges(Boolean(password) || host !== (status?.host ?? "") || port !== String(status?.port ?? 993) || username !== (status?.username ?? "") || secure !== (status?.secure ?? true) || mailbox !== (status?.mailbox ?? "INBOX"));

  const save = async (e: React.FormEvent) => {e.preventDefault(); if (outcome.isBlocked()) return;
    if (!password.trim()) {
      onError("Password er påkrævet — passwordet vises aldrig efter det er gemt.");
      return;
    }
    setSaving(true);
    try {
      await api.saveBilagsmailImapConfig(slug, {
        host,
        port: Number(port),
        username,
        password,
        secure,
        mailbox,
      }).catch(outcome.reject);
      setPassword(""); // never linger in DOM
      onDone();
    } catch (err) {
      onError(err instanceof ApiError ? err.message : "Kunne ikke gemme IMAP-config.");
    } finally {
      setPassword("");
      setSaving(false);
    }
  };

  const remove = async () => {if (outcome.isBlocked()) return;
    setSaving(true);
    try {
      await api.deleteBilagsmailImapConfig(slug).catch(outcome.reject);
      onDone();
    } catch (err) {
      onError(err instanceof ApiError ? err.message : "Kunne ikke slette config.");
      throw err;
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="card">
    {outcome.feedback}
      <h3>IMAP-konfiguration</h3>
      <p className="muted">
        Adgangen gemmes privat på serveren og medtages ikke i backup. Passwordet kan ikke hentes igen; indtast det ved hver ændring. Efter gendannelse skal en ny ejer konfigurere modtagelsen igen.
      </p>
      <p className="muted">
        Status:{" "}
        {configured ? (
          <span className="pill ok">Konfigureret</span>
        ) : (
          <span className="pill warn">Ikke konfigureret</span>
        )}
      </p>
      <form onSubmit={save}>
        <label>
          Host
          <Input disabled={outcome.blocked} type="text" value={host} onChange={(e) => setHost(e.target.value)} required />
        </label>
        <label>
          Port
          <Input disabled={outcome.blocked}
            type="number"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            required
          />
        </label>
        <label>
          Username
          <Input disabled={outcome.blocked}
            type="text"
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            required
          />
        </label>
        <label>
          Password (kun ved oprettelse/skift)
          <Input disabled={outcome.blocked}
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            placeholder={configured ? "Indtast igen for at ændre konfigurationen" : ""}
          />
        </label>
        <label>
          Mailbox
          <Input disabled={outcome.blocked}
            type="text"
            value={mailbox}
            onChange={(e) => setMailbox(e.target.value)}
          />
        </label>
        <label className="checkbox">
          <Input disabled={outcome.blocked}
            type="checkbox"
            checked={secure}
            onChange={(e) => setSecure(e.target.checked)}
          />{" "}
          IMAPS (TLS)
        </label>
        <div className="row-actions">
          <Button requiredPermission="company.admin" type="submit" className="btn primary" disabled={outcome.blocked || (saving)}>
            {saving ? "Gemmer …" : configured ? "Opdatér" : "Gem"}
          </Button>
          {configured && (
            <Button requiredPermission="company.admin" variant="danger"
              type="button"
              className="btn secondary danger"
              onClick={() => setPendingDelete(true)}
              disabled={outcome.blocked || (saving)}
            >
              Slet config
            </Button>
          )}
        </div>
      </form>
      {pendingDelete && (
        <ConfirmDialog
          title="Slet IMAP-konfigurationen"
          body={
            <p>
              Bilagsmail-polling stopper, og du skal indtaste host, port,
              brugernavn og password igen for at slå den til. Handlingen
              ændrer kun lokal konfiguration — ingen bilag slettes.
            </p>
          }
          confirmLabel="Slet konfiguration"
          confirmKind="danger"
          onConfirm={async () => {
            await remove();
          }}
          onClose={() => setPendingDelete(false)} onRefresh={onDone}
        />
      )}
    </section>
  );
}

function PollingPanel({slug,data,onDone}:{slug:string;data:CompanyBilagsmail;onDone:()=>void}) {
  const [confirm,setConfirm]=useState(false);
  const [busy,setBusy]=useState(false);
  const [message,setMessage]=useState<string>();
  const outcome=useMutationOutcome(onDone);
  return <section className="card" aria-label="Mailmodtagelse">
    {outcome.feedback}<h3>Mailmodtagelse</h3>
    {data.polling ? <p>Serverstatus: {data.polling.state} · {data.polling.enabled ? "Aktiveret" : "Ikke aktiv"}</p> : <p>Automatisk modtagelsesstatus er ikke oplyst.</p>}
    {data.polling?.lastError && <p role="alert">Seneste modtagelsesfejl: {data.polling.lastError}</p>}
    {data.pendingReviewCount != null && <p>{data.pendingReviewCount} kilder afventer gennemgang.</p>}
    <p>En mailgennemgang henter én omgang fra den konfigurerede postkasse. Den opretter ingen posteringer. Vedhæftninger skal gennemgås i den fælles indbakke før overdragelse til virksomheden. Automatisk modtagelse kræver, at serverens mailmodtagelse kører.</p>
    <div className="row-actions"><Button requiredPermission="company.admin" disabled={!data.imapConfigured || busy || outcome.blocked} onClick={()=>setConfirm(true)}>Gennemgå postkasse nu</Button><ButtonLink to={`/companies/${slug}/workspace-inbox`}>Åbn fælles dokumentindbakke</ButtonLink></div>
    {message && <p role="status">{message}</p>}
    {confirm && <ConfirmDialog title="Bekræft mailgennemgang" body={<p>Hent én omgang mail for {data.company.name} fra den konfigurerede postkasse. Originaler bevares i den fælles indbakke; ingen bilag bogføres. Statuslæsning starter aldrig modtagelse.</p>} confirmLabel="Hent mail én gang" onClose={()=>setConfirm(false)} onRefresh={onDone} onConfirm={async()=>{
      if(outcome.isBlocked() || busy) return;
      setBusy(true);
      try { await api.pollBilagsmail(slug).catch(outcome.reject);setMessage("Mailgennemgangen er afsluttet. Kontrollér status og den fælles indbakke.");onDone(); } finally {setBusy(false);}
    }} />}
  </section>;
}

function InboxPanel({
  inbox,
  currency,
}: {
  inbox: CompanyBilagsmail["inbox"];
  currency: string;
}) {
  return (
    <section className="card">
      <h3>Inbox ({inbox.length})</h3>
      <p className="muted">
        Senest overdragne mail-drop-bilag. Nye mailkilder og vedhæftninger gennemgås i den fælles dokumentindbakke.
      </p>
      {inbox.length === 0 ? (
        <p className="muted">Ingen mail-drop-bilag indlæst endnu.</p>
      ) : (
        <div className="table-scroll"><table className="table">
          <thead>
            <tr>
              <th>ID</th>
              <th>Bilag-nr.</th>
              <th>Kilde</th>
              <th>Modtaget</th>
              <th>Afsender</th>
              <th>Fakturadato</th>
              <th>Beløb inkl. moms</th>
            </tr>
          </thead>
          <tbody>
            {inbox.map((row) => (
              <tr key={row.id}>
                <td>#{row.id}</td>
                <td>{row.documentNo ?? "—"}</td>
                <td>
                  <code>{row.source}</code>
                </td>
                <td className="muted">{row.uploadDatetime ?? "—"}</td>
                <td>{row.senderName ?? "—"}</td>
                <td className="muted">{row.invoiceDate ?? "—"}</td>
                <td className="num">
                  {row.amountIncVat != null
                    ? formatKroner(row.amountIncVat, currency)
                    : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table></div>
      )}
    </section>
  );
}
