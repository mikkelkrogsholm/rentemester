import { useState } from "react";
import { Button, Dialog } from "./ui";

/** A read never releases a write lock. Only explicit human reconciliation does. */
export function UnknownMutationNotice({ onRefresh, onRelease, persistent = true, verifyPersistence }: {
  onRefresh?: () => unknown | Promise<unknown>;
  onRelease: () => void;
  persistent?: boolean;
  verifyPersistence?: () => void;
}) {
  const [refreshBusy, setRefreshBusy] = useState(false);
  const [readError, setReadError] = useState(false);
  const [confirmRelease, setConfirmRelease] = useState(false);
  async function refresh() {
    setRefreshBusy(true); setReadError(false);
    try { await onRefresh?.(); } catch { setReadError(true); }
    finally { setRefreshBusy(false); }
  }
  return <div className="banner warning" role="alert">
    {persistent ? <p>Serverens resultat kunne ikke bekræftes. Handlingen kan være gennemført. En ny skrivning er blokeret, også efter genåbning eller genindlæsning i denne fane. Kontrollér resultatet i oversigten eller revisionssporet.</p> : <><p>Skrivehandlinger er blokeret, fordi browseren ikke kan gemme beskyttelsen mod gentagelse. Behold denne fane åben, hvis en handling blev afbrudt, og kontrollér resultatet før genindlæsning.</p><p>Tillad sessionslager for appen, og kontrollér, at blokeringen kan gemmes.</p>{verifyPersistence && <Button variant="secondary" onClick={verifyPersistence}>Kontrollér browserens lager</Button>}</>}
    {onRefresh && <Button variant="secondary" disabled={refreshBusy} onClick={() => void refresh()}>Kontrollér status</Button>}
    <Button variant="secondary" disabled={refreshBusy || !persistent} onClick={() => setConfirmRelease(true)}>Resultatet er afklaret</Button>
    {readError && <p>Status kunne ikke hentes. En ny skrivning er fortsat blokeret.</p>}
    {confirmRelease && <Dialog title="Frigiv efter kontrol" onClose={() => setConfirmRelease(false)}>
      <p>Frigiv kun formularen, når du har kontrolleret, om handlingen blev gennemført. Hvis den blev gennemført, skal du beholde resultatet og undgå at gentage den. Frigivelsen sender ingen handling til serveren.</p>
      <div className="modal-actions">
        <Button variant="secondary" onClick={() => setConfirmRelease(false)}>Behold blokering</Button>
        <Button onClick={() => { setConfirmRelease(false); onRelease(); }}>Jeg har kontrolleret resultatet</Button>
      </div>
    </Dialog>}
  </div>;
}
