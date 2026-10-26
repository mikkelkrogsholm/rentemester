/**
 * Browser-side predicate for text that a sighted user can actually see.
 * Kept as source because cockpit evidence evaluates it through CDP.
 */
export const sightedUserVisibleSource = `n => {
  for (let x = n.parentElement; x; x = x.parentElement) {
    const s = getComputedStyle(x);
    if (x.tagName === 'DETAILS' && !x.open) {
      const summary = x.querySelector(':scope > summary');
      if (!summary || !summary.contains(n)) return false;
    }
    if (x.classList.contains('sr-only') || s.display === 'none' || s.visibility === 'hidden' || s.visibility === 'collapse' || s.contentVisibility === 'hidden') return false;
  }
  const r = document.createRange();
  r.selectNodeContents(n);
  return Array.from(r.getClientRects()).some(rect => rect.width || rect.height);
}`;
