import * as stylex from '@stylexjs/stylex';
import { useParams } from 'react-router-dom';
import { PageHeader } from '../components/ui';
import { ErrorState, Loading } from '../components/Feedback';
import { api, ApiError } from '../lib/api';
import { useAsync } from '../lib/useAsync';

const styles = stylex.create({ body: { whiteSpace: 'pre-wrap', overflowWrap: 'anywhere', minWidth: 0 } });
/** Protected product reference; Markdown is displayed as escaped text. */
export function KnowledgePageView() {
  const { slug, pageId = '' } = useParams();
  const state = useAsync(signal => api.knowledgePage(pageId, slug, { signal }), [slug, pageId]);
  if (state.failure instanceof ApiError && [403, 404].includes(state.failure.status)) return <ErrorState message="Referencen er ikke tilgængelig." onRetry={state.reload} />;
  if (state.loading && !state.data) return <Loading label="Henter viden og playbook…" />;
  if (!state.data) return <ErrorState message={state.error ?? 'Referencen er ikke tilgængelig.'} onRetry={state.reload} />;
  const { page } = state.data;
  return <section data-cockpit-page="knowledge-page">
    <PageHeader title={page.title} description={`${page.scope.kind === 'company' ? 'Selskabets viden' : 'Workspaceviden'} · version ${page.version}`} />
    {state.error && <p role="alert">Senest hentede version vises. {state.error}</p>}
    <article className="card" aria-label="Viden og playbook" {...stylex.props(styles.body)}>{page.bodyMarkdown}</article>
    <p className="muted">Reference: {page.pageId} · Grundlag: {page.provenance.kind}: {page.provenance.ref}</p>
    <p className="muted">Gyldig fra {page.effectiveFrom}{page.effectiveToExclusive ? ` til ${page.effectiveToExclusive} (eksklusiv)` : ''}</p>
  </section>;
}
