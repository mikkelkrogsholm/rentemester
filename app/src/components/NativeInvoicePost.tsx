import { useState } from "react";
import { api } from "../lib/api";
import { ConfirmDialog } from "./ConfirmDialog";
import { Button, Select } from "./ui";

export function NativeInvoicePost({ slug, id, number, accounts, onPosted }: { slug: string; id: number; number: string; accounts: Array<{ accountNo: string; name: string }>; onPosted: () => void }) {
  const [open, setOpen] = useState(false);
  const [revenue, setRevenue] = useState("");
  return <>
    <Button type="button" onClick={() => setOpen(true)}>Bogfør faktura</Button>
    {open && <ConfirmDialog title="Bogfør faktura" operationKey={`invoice-post:${id}`}
      body={<><p>Bogfør salget på faktura {number}. Debitorer og salgsmoms følger de bekræftede kontoroller. Fakturaen kan kun bogføres én gang.</p><label>Omsætningskonto<Select value={revenue} onChange={e => setRevenue(e.target.value)}><option value="">Vælg konto</option>{accounts.map(a => <option key={a.accountNo} value={a.accountNo}>{a.accountNo} — {a.name}</option>)}</Select></label></>}
      confirmLabel="Bogfør salget" confirmKind="danger" onRefresh={onPosted}
      onConfirm={async () => { if (!revenue) throw new Error("Vælg omsætningskonto."); await api.postNativeInvoice(slug, id, revenue); onPosted(); }}
      onClose={() => setOpen(false)} />}
  </>;
}
