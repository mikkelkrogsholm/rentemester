import type { Task, TaskBoard, TaskColumn, TaskQuery, TaskReference, TaskScope, TaskStatus, TasksView } from '../../../src/core/tasks-types';
export type { Task, TaskAssignee, TaskBoard, TaskBoardDraft, TaskBoardPreview, TaskColumn, TaskCompletionInput, TaskDeadline, TaskDraft, TaskEvent, TaskPatch, TaskPeriod, TaskProjection, TaskQuery, TaskReference, TaskReminder, TaskScope, TaskSeries, TaskSeriesDraft, TaskStatus, TasksView } from '../../../src/core/tasks-types';
export const taskStatusLabel: Record<TaskStatus, string> = { open: 'Åben', in_progress: 'I gang', waiting: 'Afventer', done: 'Færdig' };
export const taskTypeLabel: Record<Task['type'], string> = { ad_hoc: 'Ad hoc', routine: 'Rutine', obligation: 'Forpligtelse' };
export const outcomeLabel = { completed: 'Udført', not_relevant: 'Ikke relevant for perioden', cancelled: 'Aflyst', exception: 'Begrundet undtagelse' };
export const statusOptions = Object.entries(taskStatusLabel) as Array<[TaskStatus, string]>;
export function today(): string { return new Intl.DateTimeFormat('sv-SE', { timeZone: 'Europe/Copenhagen' }).format(new Date()); }
export function scopeCompanies(scope: TaskScope): string[] { return scope.kind === 'company' ? [scope.companySlug] : scope.companySlugs; }
export function scopeLabel(scope: TaskScope, view: TasksView): string {
  const names = scopeCompanies(scope).map(slug => view.companies.find(company => company.slug === slug)?.name ?? slug);
  return scope.kind === 'company' ? names[0]! : names.length ? `Fælles · ${names.join(', ')}` : 'Workspace';
}
export function canWriteScope(scope: TaskScope, view: TasksView, manage = false): boolean {
  const slugs = scopeCompanies(scope);
  if (!slugs.length) return view.canManageWorkspace;
  return slugs.every(slug => view.companies.some(company => company.slug === slug && (manage ? company.canManage : company.canWrite)));
}
export function defaultColumns(): TaskColumn[] { return statusOptions.map(([status, name]) => ({ columnId: status, name, status, isDefault: true })); }
export function boardFor(view: TasksView, companySlug?: string): TaskBoard {
  return view.boards.find(board => companySlug ? board.scope.kind === 'company' && board.scope.companySlug === companySlug : board.scope.kind === 'workspace') ?? {
    boardId: companySlug ? `company:${companySlug}` : 'workspace', version: 0, scope: companySlug ? { kind: 'company', companySlug } : { kind: 'workspace', companySlugs: [] }, columns: defaultColumns(), updatedAt: '',
  };
}
export function taskReturnTo(value: string | null): string {
  if (!value || /[\\\r\n]/.test(value)) return '/opgaver';
  try {
    const url = new URL(value, 'https://rentemester.invalid');
    if (url.origin !== 'https://rentemester.invalid' || url.hash || !/^\/(?:opgaver|companies\/[a-zA-Z0-9_-]+\/opgaver)$/.test(url.pathname)) return '/opgaver';
    return `${url.pathname}${url.search}`;
  } catch { return '/opgaver'; }
}
/** Source navigation is restricted to protected local product pages. */
export function taskSourceHref(href: string): string | null {
  try {
    const url = new URL(href, 'https://rentemester.invalid');
    return url.origin === 'https://rentemester.invalid' && /^\/companies\/[a-zA-Z0-9_-]+\//.test(url.pathname) && !/[\\\r\n]/.test(href) ? `${url.pathname}${url.search}` : null;
  } catch { return null; }
}
export function referenceHref(reference: TaskReference, scope: TaskScope): string | null {
  const slug = reference.companySlug ?? (scope.kind === 'company' ? scope.companySlug : null);
  if (reference.kind === 'knowledge') return slug ? `/companies/${encodeURIComponent(slug)}/viden/${encodeURIComponent(reference.ref)}` : `/viden/${encodeURIComponent(reference.ref)}`;
  if (!slug) return null;
  const base = `/companies/${encodeURIComponent(slug)}`;
  switch (reference.kind) {
    case 'document': return /^\d+$/.test(reference.ref) ? `${base}/bilag/${reference.ref}` : `${base}/bilag?q=${encodeURIComponent(reference.ref)}`;
    case 'bank_transaction': return `${base}/bank?transactionId=${encodeURIComponent(reference.ref)}`;
    case 'period': return `${base}/periodelas`;
    case 'approval': { const batch = /^bookkeeping-batch:(\d+)(?::\d+)?$/.exec(reference.ref); return batch ? `${base}/batchbogfoering?runId=${batch[1]}` : `${base}/kladder`; }
    case 'party': return `${base}/parter/${encodeURIComponent(reference.ref)}`;
    default: return null;
  }
}
export function taskQuery(params: URLSearchParams, slug?: string): TaskQuery {
  const status = params.get('status');
  const type = params.get('type');
  return {
    companySlugs: slug ? [slug] : params.getAll('companySlug'),
    showDone: params.get('showDone') === 'true' || status === 'done', includeArchived: params.get('includeArchived') === 'true',
    ...(status && Object.hasOwn(taskStatusLabel, status) ? { status: status as TaskStatus } : {}),
    ...(type && Object.hasOwn(taskTypeLabel, type) ? { type: type as Task['type'] } : {}),
    ...(params.get('search') ? { search: params.get('search')! } : {}),
    ...(params.get('assigneeId') && !params.get('assigneeId')!.startsWith('external:') ? { assigneeId: params.get('assigneeId')! } : {}),
    ...(params.get('from') ? { from: params.get('from')! } : {}), ...(params.get('to') ? { to: params.get('to')! } : {}),
    undated: params.get('undated') === 'true', unassigned: params.get('unassigned') === 'true',
  };
}
export function taskUrgency(task: Task, date = today()): string {
  if (task.status === 'done') return task.completion ? outcomeLabel[task.completion.outcome] : 'Færdig';
  if (task.verificationRequired) return 'Resultat skal verificeres';
  if (task.relevance === 'unknown') return 'Relevans skal afklares';
  if (task.deadline && task.deadline.date < date) return 'Forfaldent';
  if (task.deadline?.certainty === 'unconfirmed') return 'Frist skal bekræftes';
  return taskStatusLabel[task.status];
}
export function parseReferences(text: string, companySlug?: string): TaskReference[] {
  return text.split('\n').map(line => line.trim()).filter(Boolean).map(line => {
    const prefix = line.match(/^\[([a-zA-Z0-9_-]+)\]/);
    const entry = prefix ? line.slice(prefix[0].length) : line;
    const colon = entry.indexOf(':');
    const kind = entry.slice(0, colon) as TaskReference['kind'];
    if (colon < 1 || !['document', 'bank_transaction', 'period', 'approval', 'knowledge', 'external_receipt', 'party'].includes(kind) || !entry.slice(colon + 1).trim()) throw new Error('Brug én sikker reference pr. linje: document:123, knowledge:playbook eller external_receipt:kvitteringsnummer.');
    return { kind, ref: entry.slice(colon + 1).trim(), ...(prefix || companySlug ? { companySlug: prefix?.[1] ?? companySlug } : {}) };
  });
}
