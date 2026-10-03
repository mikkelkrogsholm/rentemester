import { useState } from 'react';
import { createRoot } from 'react-dom/client';
import { MemoryRouter } from 'react-router-dom';
import { Button, Dialog, PageHeader, Field, MoneyInput, Select, Textarea, Amount, DataTable, Pagination, StatusBadge, ResultReceipt, EntitySelect } from '../components/ui';
import { Banner, Loading, ErrorState } from '../components/Feedback';
import '../styles.css';

function Gallery() {
  const [dialog, setDialog] = useState(false);
  const [amount, setAmount] = useState('1.234,56');
  const [entity, setEntity] = useState('');
  const [page, setPage] = useState(1), [pageSize, setPageSize] = useState(50);
  return <main className="gallery"><PageHeader title="Komponentgalleri" description="Syntetiske eksempler. Samme komponenter, tilstande og skrifter som cockpit-appen." />
    <section><h2>Handlinger</h2><div className="row-actions">{(['primary','secondary','quiet','danger'] as const).map((variant) => <Button key={variant} variant={variant}>{variant}</Button>)}<Button disabled>Utilgængelig</Button><Button busy>Arbejder…</Button><Button onClick={() => setDialog(true)}>Åbn dialog</Button></div></section>
    <section><h2>Formular og fejl</h2><div className="gallery-fields"><Field label="Beløb" help="Indtast med dansk tusind- og decimalseparator."><MoneyInput value={amount} onValueChange={setAmount} /></Field><Field label="Regnskabsår"><Select><option>2026</option><option>2025 (arkiv)</option></Select></Field><Field label="Begrundelse" error="Angiv begrundelsen, før du fortsætter."><Textarea rows={3} /></Field><EntitySelect label="Konto" value={entity} onChange={setEntity} options={[{id:'a',label:'Syntetisk omsætningskonto'},{id:'b',label:'Syntetisk udgiftskonto'}]} /></div></section>
    <section><h2>Beløb og status</h2><DataTable caption="Syntetiske regnskabsværdier" rowKey={(row) => row.id} rows={[{id:1,value:1234567890.12,currency:'DKK'},{id:2,value:-1234.56,currency:'EUR'},{id:3,value:0,currency:'DKK'},{id:4,value:null,currency:'DKK'}]} columns={[{id:'amount',label:'Beløb',render:(row) => <Amount value={row.value} currency={row.currency} />},{id:'state',label:'Status',render:(row) => <StatusBadge label={row.value === null ? 'Ukendt' : 'Kontrolleret'} tone={row.value === null ? 'warning' : 'success'} />}]} /><Pagination total={125} page={page} pageSize={pageSize} onPageChange={setPage} onPageSizeChange={(size) => {setPageSize(size);setPage(1);}} /></section>
    <section><h2>Tilstande</h2><Loading /><ErrorState message="Serveren kunne ikke nås." onRetry={() => {}} /><Banner kind="warning">Afventer serverens bekræftelse.</Banner><ResultReceipt title="Handlingen er gennemført"><p>Den syntetiske post er registreret. Kvitteringen viser serverens resultat.</p></ResultReceipt></section>
    {dialog && <Dialog title="Gennemgå handling" onClose={() => setDialog(false)}><p>Kontrollér oplysningerne, før du fortsætter.</p><Button variant="secondary" onClick={() => setDialog(false)}>Annullér</Button><Button onClick={() => setDialog(false)}>Bekræft</Button></Dialog>}
  </main>;
}
const root = document.getElementById('root');
if (!root) throw new Error('Gallery root missing');
createRoot(root).render(<MemoryRouter><Gallery /></MemoryRouter>);
