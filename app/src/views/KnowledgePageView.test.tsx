import { afterEach, expect, test, vi } from 'bun:test';
import { cleanup, screen } from '@testing-library/react';
import { KnowledgePageView } from './KnowledgePageView';
import { renderAt } from '../test/render';
import { mockFetch } from '../test/fixtures';

const page = { pageId: 'page-synthetic', version: 3, scope: { kind: 'company', companySlug: 'acme-aps' }, title: 'Syntetisk månedsplaybook', bodyMarkdown: '# Kontrollér perioden\n<script>unsafe()</script>\n[link](javascript:unsafe())', provenance: { kind: 'user', ref: 'synthetic-author' }, effectiveFrom: '2026-01-01T00:00:00Z', effectiveToExclusive: null };
afterEach(cleanup);

test('a company playbook uses the protected scoped API and renders Markdown as escaped text', async () => {
  mockFetch({ 'GET /api/companies/acme-aps/knowledge-pages/page-synthetic': { page } });
  renderAt(<KnowledgePageView />, { route: '/companies/acme-aps/viden/page-synthetic', path: '/companies/:slug/viden/:pageId' });
  expect(await screen.findByRole('heading', { name: page.title })).toBeInTheDocument();
  expect(screen.getByRole('article', { name: 'Viden og playbook' })).toHaveTextContent('<script>unsafe()</script>');
  expect(document.querySelector('script')).toBeNull();
  expect(screen.queryByRole('link', { name: 'link' })).not.toBeInTheDocument();
  expect((fetch as unknown as ReturnType<typeof vi.fn>).mock.calls).toHaveLength(1);
});

test('a workspace reference uses the owner-protected workspace API', async () => {
  mockFetch({ 'GET /api/knowledge-pages/page-synthetic': { page: { ...page, scope: { kind: 'workspace' } } } });
  renderAt(<KnowledgePageView />, { route: '/viden/page-synthetic', path: '/viden/:pageId' });
  await screen.findByRole('heading', { name: page.title });
  expect(screen.getByText('Workspaceviden · version 3')).toBeInTheDocument();
});

test('a denied knowledge reference displays no private page body', async () => {
  globalThis.fetch = vi.fn(async () => Response.json({ ok: false, code: 'forbidden', errors: ['Referencen er ikke tilgængelig.'] }, { status: 403 })) as unknown as typeof fetch;
  renderAt(<KnowledgePageView />, { route: '/companies/acme-aps/viden/page-synthetic', path: '/companies/:slug/viden/:pageId' });
  expect(await screen.findByText('Referencen er ikke tilgængelig.')).toBeInTheDocument();
  expect(screen.queryByRole('heading', { name: page.title })).not.toBeInTheDocument();
  expect(screen.queryByRole('article')).not.toBeInTheDocument();
});
