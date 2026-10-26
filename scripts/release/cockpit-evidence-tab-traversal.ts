export const TAB_TRAVERSAL_HARD_CAP = 512;
/**
 * Chrome exposes native date/time fields as several keyboard stops. Eight covers
 * the observed locale and picker subcontrols while keeping cycle detection finite.
 */
export const MAX_CONSECUTIVE_COMPOSITE_TAB_STOPS = 8;

export type FocusStop = {
  identity: string;
  label: string;
  isTarget: boolean;
  allowsInternalTabStops?: boolean;
};

export type TabTraversalResult =
  | { reached: true; tabs: number; visited: FocusStop[] }
  | {
      reached: false;
      reason: "focus cycle" | "derived bound";
      limit: number;
      visited: FocusStop[];
    };

/** One extra Tab covers document focus; composite controls get finite internal stops. */
export function tabTraversalLimit(
  tabbableControlCount: number,
  compositeControlCount = 0,
): number {
  const internalStops = Math.max(0, compositeControlCount) * (MAX_CONSECUTIVE_COMPOSITE_TAB_STOPS - 1);
  return Math.min(
    Math.max(1, tabbableControlCount + 1 + internalStops),
    TAB_TRAVERSAL_HARD_CAP,
  );
}

export async function traverseTabs({
  limit,
  dispatchTab,
  focusedStop,
}: {
  limit: number;
  dispatchTab: () => Promise<void>;
  focusedStop: () => Promise<FocusStop>;
}): Promise<TabTraversalResult> {
  const visited: FocusStop[] = [];
  const identities = new Set<string>();
  let previousIdentity: string | undefined;
  let consecutiveStops = 0;
  for (let tab = 1; tab <= limit; tab++) {
    await dispatchTab();
    const stop = await focusedStop();
    visited.push(stop);
    if (stop.isTarget) return { reached: true, tabs: tab, visited };
    consecutiveStops = stop.identity === previousIdentity ? consecutiveStops + 1 : 1;
    const isAllowedInternalStop =
      stop.allowsInternalTabStops &&
      stop.identity === previousIdentity &&
      consecutiveStops <= MAX_CONSECUTIVE_COMPOSITE_TAB_STOPS;
    if (identities.has(stop.identity) && !isAllowedInternalStop)
      return { reached: false, reason: "focus cycle", limit, visited };
    identities.add(stop.identity);
    previousIdentity = stop.identity;
  }
  return { reached: false, reason: "derived bound", limit, visited };
}

export function tabTraversalDiagnostics(visited: FocusStop[]): string {
  return visited
    .map((stop, index) => `${index + 1}:${stop.label} [${stop.identity}]`)
    .join(" -> ");
}

export const tabbableControlCountExpression = `(()=>{
  const selector = 'a[href],area[href],button,input,select,textarea,summary,iframe,audio[controls],video[controls],[contenteditable]:not([contenteditable="false"]),[tabindex]';
  return [...document.querySelectorAll(selector)].filter((element) => {
    if (!(element instanceof HTMLElement) || element.tabIndex < 0 || element.hasAttribute('disabled') || element.closest('[inert]')) return false;
    const style = getComputedStyle(element);
    return style.visibility !== 'hidden' && style.display !== 'none' && element.getClientRects().length > 0;
  }).length;
})()`;

export const compositeTabbableControlCountExpression = `(()=>{
  const selector = 'input[type="date"],input[type="datetime-local"],input[type="month"],input[type="time"],input[type="week"]';
  return [...document.querySelectorAll(selector)].filter((element) => {
    if (!(element instanceof HTMLElement) || element.tabIndex < 0 || element.hasAttribute('disabled') || element.closest('[inert]')) return false;
    const style = getComputedStyle(element);
    return style.visibility !== 'hidden' && style.display !== 'none' && element.getClientRects().length > 0;
  }).length;
})()`;

export function focusedStopExpression(targetSelector: string): string {
  return `(()=>{
    const element = document.activeElement;
    if (!(element instanceof Element)) return { identity: 'no-active-element', label: 'no active element', isTarget: false };
    const identity = (() => {
      if (element.id) return '#' + element.id;
      const parts = [];
      for (let node = element; node && node instanceof Element; node = node.parentElement) {
        let index = 1;
        for (let sibling = node.previousElementSibling; sibling; sibling = sibling.previousElementSibling) index++;
        parts.unshift(node.tagName.toLowerCase() + ':nth-child(' + index + ')');
      }
      return parts.join('>');
    })();
    const text = (element.getAttribute('aria-label') || element.textContent || element.getAttribute('name') || '').replace(/\\s+/g, ' ').trim().slice(0, 80);
    const allowsInternalTabStops = element instanceof HTMLInputElement && ['date', 'datetime-local', 'month', 'time', 'week'].includes(element.type);
    return { identity, label: '<' + element.tagName.toLowerCase() + '>' + (text ? ' ' + JSON.stringify(text) : ''), isTarget: element.matches(${JSON.stringify(targetSelector)}), allowsInternalTabStops };
  })()`;
}
