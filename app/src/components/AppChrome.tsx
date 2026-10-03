import { useState, useEffect, useId, useRef } from 'react';
import { NavLink, useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import * as stylex from '@stylexjs/stylex';
import { colors, spacing, typography, layout } from '../design/tokens.stylex';
import { COMPANY_ROUTE_DEFINITIONS, COMPANY_TASK_AREAS, companyRouteForPath, companyYearScope } from '../company-navigation';
import { useCompanyShell, useCompanyYear, YearSelector } from './CompanyNav';
import { useAuth } from '../lib/auth-context';
import { useAsync } from '../lib/useAsync';
import { useCapabilities } from '../lib/useCapabilities';
import { api } from '../lib/api';
import { Button, Dialog, Select } from './ui';
import { ROUTE_PERMISSION_POLICY, type RoutePermission } from '../../../src/core/access-permissions';

const styles = stylex.create({
  shell: { display: 'grid', gridTemplateColumns: '248px minmax(0, 1fr)', maxWidth: layout.maxWidth, marginInline: 'auto', '@media (max-width: 1023px)': { gridTemplateColumns: 'minmax(0, 1fr)' } },
  workspace: { gridTemplateColumns: 'minmax(0, 1fr)' },
  main: { minWidth: 0, padding: spacing.lg, '@media (max-width: 639px)': { padding: spacing.md } },
  aside: { padding: spacing.md, borderRightWidth: 1, borderRightStyle: 'solid', borderRightColor: colors.border, '@media (max-width: 1023px)': { display: 'none' } },
  group: { marginBottom: spacing.md },
  groupTitle: { display: 'list-item', minHeight: 44, cursor: 'pointer', fontSize: typography.sizeSm, fontWeight: 600, color: colors.inkMuted, paddingBlock: spacing.xs },
  navLink: { display: 'block', minHeight: 44, paddingBlock: spacing.xs, paddingInline: spacing.sm, color: colors.ink, borderRadius: 4, textDecoration: 'none', ':hover': { backgroundColor: colors.paperRaised }, ':focus-visible': { outline: '2px solid', outlineColor: colors.info, outlineOffset: 2 } },
  active: { backgroundColor: colors.paperRaised, borderLeftWidth: 3, borderLeftStyle: 'solid', borderLeftColor: colors.accent, fontWeight: 600 },
  context: { display: 'flex', flexWrap: 'wrap', alignItems: 'center', justifyContent: 'space-between', gap: spacing.sm, paddingBottom: spacing.md, marginBottom: spacing.lg, borderBottomWidth: 1, borderBottomStyle: 'solid', borderBottomColor: colors.border },
  identity: { display: 'grid', gap: spacing.xxs, minWidth: 0 },
  scope: { color: colors.inkMuted, fontSize: typography.sizeSm },
  picker: { maxWidth: 'min(400px, 100%)' },
});
const restricted: Partial<Record<string, RoutePermission>> = { manage: 'company.admin' };
function TaskLinks({ onNavigate }: { onNavigate?: () => void }) {
  const location = useLocation();
  const route = companyRouteForPath(location.pathname);
  const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1];
  const [params] = useSearchParams();
  const year = params.get('year');
  const { can } = useCapabilities(slug);
  if (!slug) return null;
  const priority = ['documents', 'bank', 'batch-bookkeeping', 'drafts'];
  return <nav aria-label="Virksomhedsnavigation">{COMPANY_TASK_AREAS.map((area) => {
    const pages = COMPANY_ROUTE_DEFINITIONS.filter((entry) => entry.kind === 'page' && entry.area === area.id && can(restricted[entry.id] ?? 'company.read')).sort((a, b) => {
      const ai = priority.indexOf(a.id), bi = priority.indexOf(b.id);
      return (ai < 0 ? 99 : ai) - (bi < 0 ? 99 : bi);
    });
    if (pages.length === 0) return null;
    const destination = pages.find((entry) => entry.segment === area.destination) ?? pages[0]!;
    const suffix = year ? `?year=${encodeURIComponent(year)}` : '';
    const nestedPages = pages.filter((entry) => entry.label !== area.label);
    return <div key={area.id} {...stylex.props(styles.group)}>
      <NavLink to={`/companies/${slug}${destination.segment ? '/' + destination.segment : ''}${suffix}`} end aria-current={nestedPages.length > 0 ? false : undefined} onClick={onNavigate} {...stylex.props(styles.navLink, area.id === route?.area && styles.active)}>{area.label}</NavLink>
      {nestedPages.length > 0 && <details open={area.id === route?.area}>
        <summary {...stylex.props(styles.groupTitle)}>Sider i {area.label}</summary>
        {nestedPages.map((entry) => {
          const active = entry.id === (route?.parentId ?? route?.id);
          return <NavLink key={entry.id} to={`/companies/${slug}${entry.segment ? '/' + entry.segment : ''}${suffix}`} end onClick={onNavigate} aria-current={active ? 'page' : undefined} {...stylex.props(styles.navLink, active && styles.active)}>{entry.label}</NavLink>;
        })}
      </details>}
    </div>;
  })}</nav>;
}
export function CockpitLayout({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const route = companyRouteForPath(location.pathname);
  const styling = stylex.props(styles.shell, !route && styles.workspace);
  return <div {...styling} className={`${styling.className} cockpit-layout${route ? '' : ' workspace-layout'}`}>{route && <aside {...stylex.props(styles.aside)}><TaskLinks /></aside>}<main id="main-content" tabIndex={-1} {...stylex.props(styles.main)}>{route && <CompanyContextHeader />}{children}</main></div>;
}
function CompanyContextHeader() {
  const location = useLocation();
  const route = companyRouteForPath(location.pathname)!;
  const slug = location.pathname.match(/^\/companies\/([^/]+)/)?.[1] ?? '';
  const auth = useAuth();
  const { year, setYear } = useCompanyYear();
  const shell = useCompanyShell();
  const navigate = useNavigate();
  const [menu, setMenu] = useState(false);
  const menuId = useId();
  const switchRead = useRef<AbortController | null>(null);
  // In hosted mode the membership context already supplies names without extra access.
  const companies = useAsync<Array<{ slug: string; name: string; archived: boolean }>>((signal) => auth.hosted ? Promise.resolve(auth.context?.companies ?? []) : api.companies({ signal }), [auth.hosted, auth.context]);
  const company = companies.data?.find((entry) => entry.slug === slug);
  // biome-ignore lint/correctness/useExhaustiveDependencies: navigation closes the menu regardless of the destination.
  useEffect(() => { setMenu(false); }, [location.pathname]);
  // A late year lookup must not undo a newer company choice or navigation.
  // biome-ignore lint/correctness/useExhaustiveDependencies: pathname changes invalidate the pending destination.
  useEffect(() => () => switchRead.current?.abort(), [location.pathname]);
  async function switchCompany(nextSlug: string) {
    if (!nextSlug) return;
    switchRead.current?.abort();
    const controller = new AbortController();
    switchRead.current = controller;
    let nextYear: string | undefined;
    if (year) {
      try { const years = await api.fiscalYears(nextSlug, { signal: controller.signal }); if (years.some((entry) => entry.label === year)) nextYear = year; } catch { /* Destination displays its own authoritative access/read error. */ }
    }
    if (controller.signal.aborted) return;
    const permission = restricted[route.id] ?? 'company.read';
    const membership = auth.context?.companies.find((entry) => entry.slug === nextSlug);
    const allowed = !auth.hosted || (membership && ROUTE_PERMISSION_POLICY[membership.role].includes(permission));
    const destination = route.kind === 'page' && allowed ? route.segment : '';
    navigate(`/companies/${encodeURIComponent(nextSlug)}${destination ? '/' + destination : ''}${nextYear ? '?year=' + encodeURIComponent(nextYear) : ''}`);
  }
  const scope = companyYearScope(route.id);
  const styling = stylex.props(styles.context);
  return <header {...styling} className={`${styling.className} company-context`}><div {...stylex.props(styles.identity)}><label>Virksomhed <Select aria-label="Skift virksomhed" value={slug} {...stylex.props(styles.picker)} onChange={(event) => void switchCompany(event.target.value)}>{!company && <option value={slug}>{slug}</option>}{companies.data?.map((entry) => <option value={entry.slug} key={entry.slug}>{entry.name}{entry.archived ? ' (arkiv)' : ''}</option>)}</Select></label>{company?.archived && <span role="status">Arkiveret virksomhed · læseadgang</span>}<span {...stylex.props(styles.scope)}>{scope === 'company' ? 'Hele virksomheden' : scope === 'multi-year' ? 'Flere regnskabsår' : scope === 'vat-period' ? 'Momsperiode vælges på siden' : 'Regnskabsår'}</span></div>{scope === 'year' && shell?.controls?.slug === slug && <YearSelector years={shell.controls.years} selected={year ?? shell.controls.selected} onChange={setYear} />}<Button variant="secondary" className="mobile-menu-trigger" aria-haspopup="dialog" aria-expanded={menu} aria-controls={menuId} onClick={() => setMenu(true)}>Menu</Button>{menu && <Dialog id={menuId} title="Virksomhedsnavigation" onClose={() => setMenu(false)}><div className="navigation-dialog-actions"><Button variant="secondary" onClick={() => setMenu(false)}>Luk menu</Button></div><TaskLinks onNavigate={() => setMenu(false)} /></Dialog>}</header>;
}
export function SkipLink() { return <a href="#main-content" className="skip-link" onClick={(event) => { event.preventDefault(); document.getElementById('main-content')?.focus(); }}>Spring til indhold</a>; }
