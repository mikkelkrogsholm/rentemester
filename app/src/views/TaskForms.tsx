import { useState } from 'react';
import * as stylex from '@stylexjs/stylex';
import { Button, Dialog, Field, Input, Select, Textarea } from '../components/ui';
import { useUnsavedChanges } from '../lib/useUnsavedChanges';
import { useAsync } from "../lib/useAsync";
import { useTaskMutation } from '../lib/useTaskMutation';
import { api } from '../lib/api';
import { colors, spacing } from '../design/tokens.stylex';
import { canWriteScope, parseReferences, scopeCompanies, scopeLabel, statusOptions, today, type Task, type TaskBoard, type TaskBoardDraft, type TaskBoardPreview, type TaskDeadline, type TaskDraft, type TaskPatch, type TaskScope, type TaskSeries, type TaskSeriesDraft, type TasksView, outcomeLabel } from '../lib/tasks';

const styles = stylex.create({ form: { display: 'grid', gap: spacing.md, minWidth: 0 }, row: { display: 'flex', flexWrap: 'wrap', gap: spacing.sm, alignItems: 'end' }, columns: { display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(min(100%, 200px), 1fr))', gap: spacing.sm }, column: { display: 'grid', gap: spacing.xs, padding: spacing.sm, borderWidth: 1, borderStyle: 'solid', borderColor: colors.border }, actions: { display: 'flex', flexWrap: 'wrap', gap: spacing.sm }, full: { width: '100%' } });
export function taskAssignees(view: TasksView) { return (view as TasksView & { assignees?: Array<{ userId: string; name: string }> }).assignees ?? [{ userId: view.currentUserId, name: 'Dig' }]; }

function ScopeFields({ view, value, onChange, manage = false }: { view: TasksView; value: TaskScope | null; onChange: (scope: TaskScope) => void; manage?: boolean }) {
  const selection = value?.kind === 'company' ? value.companySlug : value ? '@workspace' : '';
  const eligibleCompanies = view.companies.filter(company => manage ? company.canManage : company.canWrite);
  return <>
    <Field label="Opgavens selskab eller fælles scope"><Select required value={selection} onChange={event => onChange(event.target.value === '@workspace' ? { kind: 'workspace', companySlugs: [] } : { kind: 'company', companySlug: event.target.value })}>
      <option value="" disabled>Vælg selskab eller fælles opgave</option>
      {eligibleCompanies.map(company => <option value={company.slug} key={company.slug}>{company.name}{company.archived ? ' · arkiveret' : ''}</option>)}
      {(view.canManageWorkspace || eligibleCompanies.length > 0) && <option value="@workspace">Fælles workspaceopgave</option>}
    </Select></Field>
    {value?.kind === 'workspace' && <fieldset><legend>{view.canManageWorkspace ? 'Berørte selskaber (valgfrit)' : 'Berørte selskaber (vælg mindst ét)'}</legend>{eligibleCompanies.map(company => <label key={company.slug}><Input type="checkbox" checked={value.companySlugs.includes(company.slug)} onChange={event => onChange({ kind: 'workspace', companySlugs: event.target.checked ? [...value.companySlugs, company.slug] : value.companySlugs.filter(slug => slug !== company.slug) })} /> {company.name} </label>)}{!view.canManageWorkspace && <p>Fælles opgaver kræver adgang til alle valgte selskaber. Vælg mindst ét selskab.</p>}</fieldset>}
  </>;
}

export function TaskEditor({ view, task, initialScope, onSaved, onClose }: { view: TasksView; task?: Task; initialScope?: TaskScope; onSaved: () => void; onClose: () => void }) {
  const [scope, setScope] = useState<TaskScope | null>(task?.scope ?? initialScope ?? null);
  const scopedMembers = useAsync(signal => scope ? api.tasks({ companySlugs: scopeCompanies(scope), includeArchived: true }, { signal }) : Promise.resolve(view), [JSON.stringify(scope)]);
  const members = taskAssignees(scopedMembers.data ?? view);
  const [title, setTitle] = useState(task?.title ?? '');
  const [description, setDescription] = useState(task?.description ?? '');
  const [nextAction, setNextAction] = useState(task?.nextAction ?? '');
  const [type, setType] = useState<Task['type']>(task?.type ?? 'ad_hoc');
  const [assigneeKind, setAssigneeKind] = useState(task?.assignee?.kind ?? 'none');
  const [assigneeId, setAssigneeId] = useState(task?.assignee?.kind === 'member' ? task.assignee.userId : view.currentUserId);
  const [externalName, setExternalName] = useState(task?.assignee?.kind === 'external' ? task.assignee.name : '');
  const [waitingOn, setWaitingOn] = useState(task?.waitingOn ?? '');
  const [workDate, setWorkDate] = useState(task?.workDate ?? '');
  const [deadlineDate, setDeadlineDate] = useState(task?.deadline?.date ?? '');
  const [deadlineKind, setDeadlineKind] = useState<TaskDeadline['kind']>(task?.deadline?.kind ?? 'internal');
  const [basis, setBasis] = useState(task?.deadline?.basis ?? '');
  const [certainty, setCertainty] = useState<TaskDeadline['certainty']>(task?.deadline?.certainty ?? 'unconfirmed');
  const [periodFrom, setPeriodFrom] = useState(task?.period?.from ?? '');
  const [periodTo, setPeriodTo] = useState(task?.period?.to ?? '');
  const [periodLabel, setPeriodLabel] = useState(task?.period?.label ?? '');
  const [references, setReferences] = useState(task?.references.map(ref => `${ref.companySlug ? `[${ref.companySlug}]` : ''}${ref.kind}:${ref.ref}`).join('\n') ?? '');
  const [evidenceRequired, setEvidenceRequired] = useState(task?.evidenceRequired ?? false);
  const [relevance, setRelevance] = useState<Task['relevance']>(task?.relevance ?? 'relevant');
  const [verificationRequired, setVerificationRequired] = useState(task?.verificationRequired ?? false);
  const [verificationReason, setVerificationReason] = useState('');
  const clarifyingExternalResult = !!task?.verificationRequired && !task.source && !verificationRequired;
  const [version, setVersion] = useState(task?.version);
  const [validation, setValidation] = useState<string | null>(null);
  const write = useTaskMutation(onSaved, task ? `edit:${task.taskId}` : 'create', onClose);
  const formSnapshot = JSON.stringify({ scope, title, description, nextAction, type, assigneeKind, assigneeId, externalName, waitingOn, workDate, deadlineDate, deadlineKind, basis, certainty, periodFrom, periodTo, periodLabel, references, evidenceRequired, relevance, verificationRequired, verificationReason });
  const [initialSnapshot] = useState(formSnapshot);
  const markSaved = useUnsavedChanges(formSnapshot !== initialSnapshot);
  async function save() {
    setValidation(null);
    if (!scope || !title.trim()) return;
    try {
      if (!canWriteScope(scope, view)) throw new Error('Du kan ikke ændre opgaver i det valgte scope.');
      const assignee = assigneeKind === 'member' ? { kind: 'member' as const, userId: assigneeId, name: members.find(member => member.userId === assigneeId)?.name ?? task?.assignee?.name ?? assigneeId } : assigneeKind === 'external' ? { kind: 'external' as const, name: externalName.trim() } : null;
      const parsedReferences = parseReferences(references, scope.kind === 'company' ? scope.companySlug : undefined);
      if (clarifyingExternalResult && (relevance !== 'relevant' || parsedReferences.length === 0 || !verificationReason.trim())) throw new Error('Verificér det eksterne resultat: vælg Relevant, tilføj et bevis og forklar, hvad du har kontrolleret.');
      const documentedDescription = clarifyingExternalResult ? `${description.trim()}\n\nVerifikation: ${verificationReason.trim()}`.trim() : description;
      const patch: TaskPatch = { title: title.trim(), scope, description: documentedDescription, nextAction, assignee, waitingOn, workDate: workDate || null,
        deadline: deadlineDate ? { date: deadlineDate, kind: deadlineKind, basis, certainty } : null,
        period: periodFrom && periodTo ? { from: periodFrom, to: periodTo, label: periodLabel || `${periodFrom}–${periodTo}` } : null,
        references: parsedReferences, evidenceRequired: !!task?.evidenceRequired || evidenceRequired, relevance, verificationRequired };
      const result = await write.run<{ task: Task }>(task ? `/api/tasks/${encodeURIComponent(task.taskId)}/update` : '/api/tasks', task ? { patch, expectedVersion: version } : { ...patch, type });
      if (result) { markSaved(); onClose(); }
    } catch (cause) { setValidation(cause instanceof Error ? cause.message : 'Kontrollér formularen.'); }
  }
  return <Dialog title={task ? 'Redigér opgave' : 'Opret opgave'} onClose={onClose} busy={write.busy}><form {...stylex.props(styles.form)} onSubmit={event => { event.preventDefault(); void save(); }}>
    {write.feedback}{validation && <p role="alert">{validation}</p>}
    {task && task.version !== version && <div className="banner warning"><p>Opgaven har nu version {task.version}. Dine ændringer er bevaret. Kontrollér den aktuelle opgave før ny gemning.</p><Button variant="secondary" onClick={() => setVersion(task.version)}>Brug den aktuelle version som grundlag</Button></div>}
    {task?.seriesId && <p>Du ændrer kun denne forekomst. Rutinen og andre perioders opgaver bevares.</p>}
    <Field label="Titel"><Input required value={title} maxLength={240} onChange={event => setTitle(event.target.value)} /></Field>
    <ScopeFields view={view} value={scope} onChange={setScope} />
    <details open={!!task}><summary>Planlægning og dokumentation</summary><div {...stylex.props(styles.form)}>
      {!task && <Field label="Opgavetype"><Select value={type} onChange={event => setType(event.target.value as Task['type'])}><option value="ad_hoc">Ad hoc</option><option value="routine">Rutine</option><option value="obligation">Forpligtelse</option></Select></Field>}
      <Field label="Beskrivelse"><Textarea value={description} onChange={event => setDescription(event.target.value)} /></Field>
      <Field label="Næste handling"><Textarea value={nextAction} onChange={event => setNextAction(event.target.value)} /></Field>
      <Field label="Ansvarlig"><Select value={assigneeKind} onChange={event => setAssigneeKind(event.target.value)}><option value="none">Ikke tildelt</option><option value="member">Medlem</option><option value="external">Ekstern ansvarlig</option></Select></Field>
      {assigneeKind === 'member' && <Field label="Vælg medlem"><Select value={assigneeId} onChange={event => setAssigneeId(event.target.value)}>{members.map(member => <option key={member.userId} value={member.userId}>{member.name}</option>)}</Select></Field>}
      {assigneeKind === 'external' && <Field label="Ekstern ansvarligs navn" help="Tildeling giver ingen adgang og sender ingen besked."><Input required value={externalName} onChange={event => setExternalName(event.target.value)} /></Field>}
      <Field label="Afventer svar fra"><Input value={waitingOn} onChange={event => setWaitingOn(event.target.value)} /></Field>
      <Field label="Arbejdsdato" help="Ændring af arbejdsdato flytter ikke fristen."><Input type="date" value={workDate} onChange={event => setWorkDate(event.target.value)} /></Field>
      <Field label="Frist"><Input type="date" value={deadlineDate} onChange={event => setDeadlineDate(event.target.value)} /></Field>
      {deadlineDate && <><Field label="Fristens art"><Select value={deadlineKind} onChange={event => setDeadlineKind(event.target.value as TaskDeadline['kind'])}><option value="internal">Intern</option><option value="agreement">Aftalt</option><option value="statutory">Myndighedsfrist</option></Select></Field><Field label="Grundlag for fristen"><Textarea required value={basis} onChange={event => setBasis(event.target.value)} /></Field><Field label="Fristens sikkerhed"><Select value={certainty} onChange={event => setCertainty(event.target.value as TaskDeadline['certainty'])}><option value="unconfirmed">Skal bekræftes</option><option value="confirmed">Bekræftet grundlag</option></Select></Field></>}
      <div {...stylex.props(styles.columns)}><Field label="Periode fra"><Input type="date" value={periodFrom} required={!!periodTo} onChange={event => setPeriodFrom(event.target.value)} /></Field><Field label="Periode til"><Input type="date" min={periodFrom} value={periodTo} required={!!periodFrom} onChange={event => setPeriodTo(event.target.value)} /></Field><Field label="Periodenavn"><Input value={periodLabel} onChange={event => setPeriodLabel(event.target.value)} /></Field></div>
      <ReferenceField value={references} onChange={setReferences} />
      <label><Input type="checkbox" checked={!!task?.evidenceRequired || evidenceRequired} disabled={!!task?.evidenceRequired} onChange={event => setEvidenceRequired(event.target.checked)} /> Kræv afslutningsbevis</label>{task?.evidenceRequired && <p>Kravet om afslutningsbevis er allerede fastlagt og bevares.</p>}
      <Field label="Relevans"><Select value={relevance} onChange={event => setRelevance(event.target.value as Task['relevance'])}><option value="relevant">Relevant</option><option value="unknown">Relevans skal afklares</option><option value="not_relevant">Ikke relevant</option></Select></Field>
      <label><Input type="checkbox" checked={verificationRequired} disabled={task?.source?.state === 'unknown'} onChange={event => setVerificationRequired(event.target.checked)} /> Resultat skal verificeres før nyt forsøg</label>
      {task?.source?.state === 'unknown' ? <p>Den aktuelle kilde skal kontrolleres. Opdatér fra kilder efter afklaringen.</p> : task?.verificationRequired && <p>Et uklart eksternt resultat kan kun afklares med relevansen Relevant, en sikker bevisreference og en forklaring i beskrivelsen.</p>}
      {clarifyingExternalResult && <Field label="Hvad har du verificeret?" help="Forklar det kontrollerede resultat. Forklaringen gemmes i opgavens beskrivelse sammen med bevisreferencerne."><Textarea required value={verificationReason} onChange={event => setVerificationReason(event.target.value)} /></Field>}
    </div></details>
    <div {...stylex.props(styles.actions)}><Button type="submit" busy={write.busy} disabled={write.blocked || !scope || !canWriteScope(scope, view) || !title.trim() || (task && task.version !== version)}>Gem opgave</Button><Button variant="secondary" disabled={write.busy} onClick={onClose}>Annullér</Button></div>
  </form></Dialog>;
}

export function ReferenceField({ value, onChange }: { value: string; onChange: (text: string) => void }) {
  return <Field label="Sikre referencer" help="Én pr. linje, fx document:123, knowledge:playbook eller external_receipt:kvitteringsnummer. Fælles opgaver kan bruge [selskabsreference]document:123. Brug referencer; skriv ikke CPR-numre eller bankoplysninger."><Textarea value={value} onChange={event => onChange(event.target.value)} /></Field>;
}

export function CompletionEditor({ task, reopen = false, onSaved, onClose }: { task: Task; reopen?: boolean; onSaved: () => void; onClose: () => void }) {
  const [outcome, setOutcome] = useState<keyof typeof outcomeLabel>('completed');
  const [note, setNote] = useState('');
  const [references, setReferences] = useState('');
  const [validation, setValidation] = useState<string | null>(null);
  const [version, setVersion] = useState(task.version);
  const write = useTaskMutation(onSaved, `${reopen ? 'reopen' : 'complete'}:${task.taskId}`, onClose);
  async function save() {
    try {
      const refs = parseReferences(references, task.scope.kind === 'company' ? task.scope.companySlug : undefined);
      if (!reopen && task.evidenceRequired && outcome === 'completed' && refs.length === 0) { setValidation('Denne opgave kræver et afslutningsbevis. Tilføj en reference eller vælg en begrundet undtagelse.'); return; }
      const result = await write.run(`/api/tasks/${encodeURIComponent(task.taskId)}/${reopen ? 'reopen' : 'complete'}`, { ...(reopen ? { reason: note } : { outcome, note, references: refs }), expectedVersion: version });
      if (result) onClose();
    } catch (cause) { setValidation(cause instanceof Error ? cause.message : 'Kontrollér referencerne.'); }
  }
  return <Dialog title={reopen ? 'Genåbn opgave' : 'Afslut opgave'} busy={write.busy} onClose={onClose}><form {...stylex.props(styles.form)} onSubmit={event => { event.preventDefault(); void save(); }}>
    {write.feedback}{validation && <p role="alert">{validation}</p>}
    <VersionNotice current={task.version} expected={version} onAccept={() => setVersion(task.version)} />
    <p>{task.title} · Tidligere dokumentation bevares.</p>
    {!reopen && <Field label="Afslutningsudfald"><Select value={outcome} onChange={event => setOutcome(event.target.value as keyof typeof outcomeLabel)}>{Object.entries(outcomeLabel).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></Field>}
    <Field label={reopen ? 'Årsag til genåbning' : 'Afslutningsnote eller begrundelse'}><Textarea required value={note} onChange={event => setNote(event.target.value)} /></Field>
    {!reopen && <ReferenceField value={references} onChange={setReferences} />}
    {task.source && <p>Den aktuelle kilde kontrolleres før afslutning. En statusændring udfører ikke kildehandlingen.</p>}
    <Button type="submit" busy={write.busy} disabled={write.blocked || task.version !== version || !note.trim()}>{reopen ? 'Genåbn med årsag' : 'Gem afslutning'}</Button>
  </form></Dialog>;
}

export function SeriesEditor({ view, series, initialScope, onSaved, onClose }: { view: TasksView; series?: TaskSeries; initialScope?: TaskScope; onSaved: () => void; onClose: () => void }) {
  const [title, setTitle] = useState(series?.title ?? '');
  const [scope, setScope] = useState<TaskScope | null>(series?.scope ?? initialScope ?? null);
  const [cadence, setCadence] = useState<TaskSeries['cadence']>(series?.cadence ?? 'month');
  const [every, setEvery] = useState(String(series?.every ?? 1));
  const [anchor, setAnchor] = useState<TaskSeries['anchor']>(series?.anchor ?? 'calendar');
  const [fiscalMonth, setFiscalMonth] = useState(String(series?.fiscalYearStartMonth ?? 1));
  const [start, setStart] = useState(series?.startDate ?? today());
  const [end, setEnd] = useState(series?.endDate ?? '');
  const [workOffset, setWorkOffset] = useState(String(series?.workDayOffset ?? 0));
  const [deadlineOffset, setDeadlineOffset] = useState(series?.deadlineDayOffset == null ? '' : String(series.deadlineDayOffset));
  const [deadlineKind, setDeadlineKind] = useState<TaskDeadline['kind']>(series?.template.deadline?.kind ?? 'internal');
  const [deadlineBasis, setDeadlineBasis] = useState(series?.template.deadline?.basis ?? '');
  const [certainty, setCertainty] = useState<TaskDeadline['certainty']>(series?.template.deadline?.certainty ?? 'unconfirmed');
  const [relevance, setRelevance] = useState<TaskSeries['relevance']>(series?.relevance ?? 'relevant');
  const [evidenceRequired, setEvidenceRequired] = useState(series?.template.evidenceRequired ?? false);
  const [active, setActive] = useState(series?.active ?? true);
  const [action, setAction] = useState(series?.template.nextAction ?? '');
  const [version, setVersion] = useState(series?.version ?? 0);
  const write = useTaskMutation(onSaved, `series:${series?.seriesId ?? 'new'}`, onClose);
  const formSnapshot = JSON.stringify({ title, scope, cadence, every, anchor, fiscalMonth, start, end, workOffset, deadlineOffset, deadlineKind, deadlineBasis, certainty, relevance, evidenceRequired, active, action });
  const [initialSnapshot] = useState(formSnapshot);
  const markSaved = useUnsavedChanges(formSnapshot !== initialSnapshot);
  async function save() {
    if (!scope || !canWriteScope(scope, view, true)) return;
    const prior = series?.template;
    const template: TaskDraft = { title: title.trim(), scope, type: 'routine', description: prior?.description, assignee: prior?.assignee, waitingOn: prior?.waitingOn, workDate: prior?.workDate, period: prior?.period, references: prior?.references, relevance: prior?.relevance, verificationRequired: prior?.verificationRequired, nextAction: action, evidenceRequired: !!series?.template.evidenceRequired || evidenceRequired, deadline: deadlineOffset ? { date: start, kind: deadlineKind, basis: deadlineBasis, certainty } : null };
    const draft: TaskSeriesDraft = { seriesId: series?.seriesId ?? crypto.randomUUID(), title: title.trim(), scope, template, cadence, every: Number(every), anchor, startDate: start, endDate: end || null, fiscalYearStartMonth: Number(fiscalMonth), workDayOffset: Number(workOffset), deadlineDayOffset: deadlineOffset ? Number(deadlineOffset) : null, relevance, active };
    const result = await write.run('/api/task-series', { series: draft, expectedVersion: version });
    if (result) { markSaved(); onClose(); }
  }
  return <Dialog title={series ? 'Redigér fremtidige gentagelser' : 'Opret rutine'} onClose={onClose} busy={write.busy}><form {...stylex.props(styles.form)} onSubmit={event => { event.preventDefault(); void save(); }}>
    {write.feedback}<VersionNotice current={series?.version ?? 0} expected={version} onAccept={() => setVersion(series?.version ?? 0)} /><p>Allerede oprettede og afsluttede forekomster bevares. Ændringer gælder fremtidige forekomster.</p>
    <Field label="Rutinens titel"><Input required maxLength={240} value={title} onChange={event => setTitle(event.target.value)} /></Field>
    <ScopeFields view={view} value={scope} onChange={setScope} manage />
    <Field label="Næste handling"><Textarea value={action} onChange={event => setAction(event.target.value)} /></Field>
    <div {...stylex.props(styles.columns)}><Field label="Gentagelse"><Select value={cadence} onChange={event => setCadence(event.target.value as TaskSeries['cadence'])}><option value="month">Måned</option><option value="quarter">Kvartal</option><option value="year">År</option><option value="custom">Tilpasset (måneder)</option></Select></Field><Field label="Hvert antal perioder"><Input required type="number" min={1} max={120} value={every} onChange={event => setEvery(event.target.value)} /></Field></div>
    <Field label="Periodernes grundlag"><Select value={anchor} onChange={event => setAnchor(event.target.value as TaskSeries['anchor'])}><option value="calendar">Kalenderår</option><option value="fiscal">Regnskabsår</option></Select></Field>
    {anchor === 'fiscal' && <Field label="Regnskabsårets startmåned"><Input required type="number" min={1} max={12} value={fiscalMonth} onChange={event => setFiscalMonth(event.target.value)} /></Field>}
    <div {...stylex.props(styles.columns)}><Field label="Starter"><Input type="date" required value={start} onChange={event => setStart(event.target.value)} /></Field><Field label="Slutter (valgfrit)"><Input type="date" min={start} value={end} onChange={event => setEnd(event.target.value)} /></Field></div>
    <Field label="Arbejdsdato: dage efter periodens start"><Input type="number" required value={workOffset} onChange={event => setWorkOffset(event.target.value)} /></Field>
    <Field label="Frist: dage efter periodens start (valgfrit)"><Input type="number" value={deadlineOffset} onChange={event => setDeadlineOffset(event.target.value)} /></Field>
    {deadlineOffset && <><Field label="Fristens art"><Select value={deadlineKind} onChange={event => setDeadlineKind(event.target.value as TaskDeadline['kind'])}><option value="internal">Intern</option><option value="agreement">Aftalt</option><option value="statutory">Myndighedsfrist</option></Select></Field><Field label="Fristgrundlag" help="En fast forskydning er en planlægningsregel. Myndighedsfrister kræver dokumenteret grundlag for den konkrete periode."><Textarea required value={deadlineBasis} onChange={event => setDeadlineBasis(event.target.value)} /></Field><Field label="Fristbekræftelse"><Select value={certainty} onChange={event => setCertainty(event.target.value as TaskDeadline['certainty'])}><option value="unconfirmed">Skal bekræftes</option><option value="confirmed">Bekræftet grundlag</option></Select></Field></>}
    <Field label="Relevans for perioden"><Select value={relevance} onChange={event => setRelevance(event.target.value as TaskSeries['relevance'])}><option value="relevant">Altid relevant</option><option value="unknown">Skal afklares</option><option value="activity">Kun ved relevant aktivitet</option></Select></Field>
    <label><Input type="checkbox" checked={!!series?.template.evidenceRequired || evidenceRequired} disabled={!!series?.template.evidenceRequired} onChange={event => setEvidenceRequired(event.target.checked)} /> Kræv afslutningsbevis</label>{series?.template.evidenceRequired && <p>Kravet om afslutningsbevis bevares for rutinen.</p>}
    <label><Input type="checkbox" checked={active} onChange={event => setActive(event.target.checked)} /> Aktiv gentagelse</label>
    <Button type="submit" busy={write.busy} disabled={write.blocked || (series?.version ?? 0) !== version || !scope || !canWriteScope(scope, view, true) || !title.trim()}>Gem rutine</Button>
  </form></Dialog>;
}

export function BoardEditor({ view, board, onSaved, onClose }: { view: TasksView; board: TaskBoard; onSaved: () => void; onClose: () => void }) {
  const [columns, setColumns] = useState(board.columns.map(column => ({ ...column })));
  const [relocations, setRelocations] = useState<Record<string, string>>({});
  const [preview, setPreview] = useState<TaskBoardPreview | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [previewing, setPreviewing] = useState(false);
  const [version, setVersion] = useState(board.version);
  const write = useTaskMutation(onSaved, `board:${board.boardId}`, onClose);
  const removed = board.columns.filter(column => !columns.some(current => current.columnId === column.columnId));
  const draft: TaskBoardDraft = { boardId: board.boardId, scope: board.scope, columns, relocations, ...(preview ? { previewHash: preview.previewHash } : {}) };
  function change(next: typeof columns) { setColumns(next); setPreview(null); }
  async function showPreview() {
    setPreviewing(true); setError(null);
    try { setPreview((await api.previewTaskBoard({ ...draft, previewHash: undefined })).preview); } catch (cause) { setError(cause instanceof Error ? cause.message : 'Kunne ikke vise konsekvenser.'); } finally { setPreviewing(false); }
  }
  async function save() { if (!preview) return; const result = await write.run('/api/task-boards', { board: draft, expectedVersion: version }); if (result) onClose(); }
  return <Dialog title="Tilpas kolonner" onClose={onClose} busy={write.busy}><div {...stylex.props(styles.form)}>
    {write.feedback}<VersionNotice current={board.version} expected={version} onAccept={() => { setVersion(board.version); setPreview(null); }} />{error && <p role="alert">{error}</p>}<p>{scopeLabel(board.scope, view)} · Hver kolonne har en fælles statusbetydning.</p>
    {columns.map((column, index) => <fieldset key={column.columnId} {...stylex.props(styles.column)}><legend>Kolonne {index + 1}</legend>
      <Field label={`Kolonnenavn ${index + 1}`}><Input value={column.name} onChange={event => change(columns.map((item, i) => i === index ? { ...item, name: event.target.value } : item))} /></Field>
      <Field label={`Statusbetydning ${index + 1}`}><Select value={column.status} onChange={event => change(columns.map((item, i) => i === index ? { ...item, status: event.target.value as typeof item.status, isDefault: false } : item))}>{statusOptions.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</Select></Field>
      <label><Input type="checkbox" checked={column.isDefault} onChange={event => change(columns.map((item, i) => item.status === column.status ? { ...item, isDefault: i === index && event.target.checked } : item))} /> Synligt standardvalg for denne status</label>
      <div {...stylex.props(styles.actions)}><Button variant="secondary" disabled={index === 0} aria-label={`Flyt ${column.name} op`} onClick={() => { const next = [...columns]; [next[index - 1], next[index]] = [next[index]!, next[index - 1]!]; change(next); }}>Op</Button><Button variant="secondary" disabled={index === columns.length - 1} aria-label={`Flyt ${column.name} ned`} onClick={() => { const next = [...columns]; [next[index], next[index + 1]] = [next[index + 1]!, next[index]!]; change(next); }}>Ned</Button><Button variant="danger" onClick={() => change(columns.filter((_, i) => i !== index))} aria-label={`Fjern ${column.name}`}>Fjern</Button></div>
    </fieldset>)}
    <Button variant="secondary" onClick={() => change([...columns, { columnId: crypto.randomUUID(), name: 'Ny kolonne', status: 'waiting', isDefault: false }])}>Tilføj kolonne</Button>
    {removed.map(column => <Field key={column.columnId} label={`Flyt opgaver fra ${column.name} til`}><Select value={relocations[column.columnId] ?? ''} onChange={event => { setRelocations({ ...relocations, [column.columnId]: event.target.value }); setPreview(null); }}><option value="">Vælg målkolonne</option>{columns.map(target => <option key={target.columnId} value={target.columnId}>{target.name}</option>)}</Select></Field>)}
    <Button variant="secondary" busy={previewing} disabled={write.blocked || columns.length === 0 || columns.some(column => !column.name.trim()) || removed.some(column => !relocations[column.columnId])} onClick={() => void showPreview()}>Vis konsekvenser</Button>
    {preview && <div className="banner warning"><p>{preview.affectedTaskIds.length} opgaver berøres. {preview.statusChanges.length} opgaver skifter fælles status.</p>{preview.statusChanges.length > 0 && <ul>{preview.statusChanges.map(change => <li key={change.taskId}>{view.tasks.find(task => task.taskId === change.taskId)?.title ?? change.taskId}: {statusOptions.find(([status]) => status === change.from)?.[1]} → {statusOptions.find(([status]) => status === change.to)?.[1]}</li>)}</ul>}<Button busy={write.busy} disabled={write.blocked || board.version !== version} onClick={() => void save()}>Anvend viste ændringer</Button></div>}
  </div></Dialog>;
}

export function ReminderEditor({ task, view, onSaved, onClose }: { task: Task; view: TasksView; onSaved: () => void; onClose: () => void }) {
  const [selected, setSelected] = useState(task.reminders[0]?.reminderId ?? 'new');
  const existing = task.reminders.find(reminder => reminder.reminderId === selected);
  return <Dialog title="Påmindelser i Rentemester" onClose={onClose}><div {...stylex.props(styles.form)}>
    <p>Kanalen er Rentemester. Tildeling aktiverer ikke påmindelser.</p>
    <Field label="Vælg påmindelse"><Select value={selected} onChange={event => setSelected(event.target.value)}><option value="new">Ny påmindelse</option>{task.reminders.map(reminder => <option key={reminder.reminderId} value={reminder.reminderId}>{reminder.kind === 'deadline' ? 'Fristvarsel' : 'Opfølgning'} · {taskAssignees(view).find(member => member.userId === reminder.recipientId)?.name ?? reminder.recipientId}</option>)}</Select></Field>
    <ReminderForm key={selected} task={task} view={view} existing={existing} onSaved={onSaved} onClose={onClose} />
  </div></Dialog>;
}
function ReminderForm({ task, view, existing, onSaved, onClose }: { task: Task; view: TasksView; existing: Task['reminders'][number] | undefined; onSaved: () => void; onClose: () => void }) {
  const [enabled, setEnabled] = useState(existing?.enabled ?? false);
  const [kind, setKind] = useState(existing?.kind ?? (task.deadline ? 'deadline' : 'follow_up'));
  const [recipient, setRecipient] = useState(existing?.recipientId ?? view.currentUserId);
  const [days, setDays] = useState(String(existing?.daysBefore ?? 7));
  const [date, setDate] = useState(existing?.followUpDate ?? today());
  const [frequency, setFrequency] = useState(existing?.frequency ?? 'once');
  const [timeZone, setTimeZone] = useState(existing?.timeZone ?? 'Europe/Copenhagen');
  const [version, setVersion] = useState(task.version);
  const write = useTaskMutation(onSaved, `reminder:${task.taskId}`, onClose);
  async function save() {
    const reminder = { reminderId: existing?.reminderId ?? crypto.randomUUID(), recipientId: recipient, enabled, kind, ...(kind === 'deadline' ? { daysBefore: Number(days) } : { followUpDate: date }), frequency, timeZone };
    if (await write.run(`/api/tasks/${encodeURIComponent(task.taskId)}/reminder`, { reminder, expectedVersion: version })) onClose();
  }
  return <form {...stylex.props(styles.form)} onSubmit={event => { event.preventDefault(); void save(); }}>
    {write.feedback}<VersionNotice current={task.version} expected={version} onAccept={() => setVersion(task.version)} /><p role="status">{view.runtime.running ? 'Påmindelsesruntime kører.' : 'Påmindelsesruntime er stoppet. Planen kan gemmes, men leveringen er inaktiv.'}</p>
    <label><Input type="checkbox" checked={enabled} onChange={event => setEnabled(event.target.checked)} /> Aktivér denne påmindelsesplan</label>
    <Field label="Modtager"><Select value={recipient} onChange={event => setRecipient(event.target.value)}>{taskAssignees(view).map(member => <option key={member.userId} value={member.userId}>{member.name}</option>)}</Select></Field>
    <Field label="Påmindelsestype"><Select value={kind} onChange={event => setKind(event.target.value as typeof kind)}><option value="follow_up">Opfølgningsdato</option>{task.deadline && <option value="deadline">Før frist</option>}</Select></Field>
    {kind === 'deadline' ? <Field label="Dage før frist"><Input type="number" min={0} max={365} required value={days} onChange={event => setDays(event.target.value)} /></Field> : <Field label="Opfølgningsdato"><Input type="date" required value={date} onChange={event => setDate(event.target.value)} /></Field>}
    <Field label="Hyppighed"><Select value={frequency} onChange={event => setFrequency(event.target.value as typeof frequency)}><option value="once">Én gang</option><option value="daily">Dagligt indtil afslutning</option></Select></Field>
    <Field label="Tidszone"><Input required value={timeZone} onChange={event => setTimeZone(event.target.value)} /></Field>
    <Button type="submit" busy={write.busy} disabled={write.blocked || task.version !== version}>Gem påmindelsesplan</Button>
  </form>;
}

function VersionNotice({ current, expected, onAccept }: { current: number; expected: number; onAccept: () => void }) {
  return current !== expected ? <div className="banner warning"><p>En anden ændring er registreret (version {current}). Din indtastning er bevaret. Kontrollér den aktuelle version før ny gemning.</p><Button variant="secondary" onClick={onAccept}>Brug den aktuelle version som grundlag</Button></div> : null;
}
