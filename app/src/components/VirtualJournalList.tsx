import { cockpitStyles } from "../design/cockpit.stylex";
import * as stylex from "@stylexjs/stylex";
import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { CompanyJournal } from "../lib/types";

type Entry = CompanyJournal["entries"][number];
const virtualStyles = stylex.create({
  list: (height: number) => ({ height, position: "relative", display: "block" }),
  row: (start: number) => ({ position: "absolute", top: 0, left: 0, width: "100%", transform: `translateY(${start}px)` }),
});
export const journalEntryKey = (entry: Entry) => entry.id ?? entry.sourceId!;
export type VirtualJournalRowProps = {
  ref: (node: HTMLLIElement | null) => void;
  "data-index": number;
  "aria-posinset": number;
  "aria-setsize": number;
  xstyle: stylex.StyleXStyles;
};

/** Only large journals use this measured window. Expanded rows retain their own
 * draft state; rangeExtractor keeps those rows (and the focused row) mounted. */
export function VirtualJournalList({ entries, renderEntry }: {
  entries: Entry[];
  renderEntry: (entry: Entry, props: VirtualJournalRowProps, onOpenChange: (open: boolean) => void) => ReactNode;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const [expanded, setExpanded] = useState<Set<string | number>>(() => new Set());
  const [focused, setFocused] = useState<string | number | null>(null);
  const pendingFocus = useRef<number | null>(null);
  const getItemKey = useCallback((index: number) => journalEntryKey(entries[index]!), [entries]);
  const rangeExtractor = useCallback((range: Parameters<typeof defaultRangeExtractor>[0]) => {
    const indexes = new Set(defaultRangeExtractor(range));
    entries.forEach((entry, index) => {
      const key = journalEntryKey(entry);
      if (expanded.has(key) || focused === key) indexes.add(index);
    });
    return [...indexes].sort((a, b) => a - b);
  }, [entries, expanded, focused]);
  const virtualizer = useVirtualizer({
    count: entries.length, getScrollElement: () => viewport.current,
    estimateSize: () => 54, getItemKey, overscan: 5, gap: 4, rangeExtractor,
  });
  const items = virtualizer.getVirtualItems();
  useLayoutEffect(() => {
    const index = pendingFocus.current;
    if (index === null) return;
    const button = viewport.current?.querySelector<HTMLButtonElement>(`[data-index="${index}"] [data-journal-summary]`);
    if (button) {
      pendingFocus.current = null;
      button.focus({ preventScroll: true });
    }
  });
  // Filtering must start at a useful position, without remounting retained rows.
  // biome-ignore lint/correctness/useExhaustiveDependencies: replacing the filtered entry set must reset the viewport even when the virtualizer instance is stable.
  useLayoutEffect(() => { virtualizer.scrollToOffset(0); }, [entries, virtualizer]);
  return <div ref={viewport} {...stylex.props(...[cockpitStyles.journalVirtualViewport])} role="region" aria-label="Posteringer — brug pil op/ned, Page Up/Down, Home/End på en postering"
    onFocusCapture={event => {
      const row = (event.target as HTMLElement).closest<HTMLElement>("[data-index]");
      if (row) setFocused(getItemKey(Number(row.dataset.index)));
    }}
    onBlurCapture={event => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocused(null);
    }}
    onKeyDown={event => {
      if (!(event.target as HTMLElement).matches("[data-journal-summary]")) return;
      const index = Number((event.target as HTMLElement).closest<HTMLElement>("[data-index]")?.dataset.index);
      const step = Math.max(1, Math.floor((viewport.current?.clientHeight ?? 540) / 54));
      const next = ({ ArrowDown: index + 1, ArrowUp: index - 1, PageDown: index + step, PageUp: index - step, Home: 0, End: entries.length - 1 } as Record<string, number>)[event.key];
      if (next === undefined) return;
      event.preventDefault();
      const target = Math.max(0, Math.min(entries.length - 1, next));
      pendingFocus.current = target;
      setFocused(getItemKey(target));
      virtualizer.scrollToIndex(target, { align: "auto" });
    }}>
    <ul {...stylex.props(cockpitStyles.entryList, virtualStyles.list(virtualizer.getTotalSize()))}>
      {items.map(item => renderEntry(entries[item.index]!, {
        ref: virtualizer.measureElement, "data-index": item.index,
        "aria-posinset": item.index + 1, "aria-setsize": entries.length,
        xstyle: virtualStyles.row(item.start),
      }, open => setExpanded(previous => {
        const next = new Set(previous);
        const key = getItemKey(item.index);
        if (open) next.add(key); else next.delete(key);
        return next;
      })))}
    </ul>
  </div>;
}
