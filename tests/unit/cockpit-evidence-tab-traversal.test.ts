import { expect, test } from "bun:test";
import {
  TAB_TRAVERSAL_HARD_CAP,
  tabTraversalDiagnostics,
  tabTraversalLimit,
  traverseTabs,
} from "../../scripts/release/cockpit-evidence-tab-traversal";

test("derives a Tab bound above the former 24-control limit", async () => {
  expect(tabTraversalLimit(30)).toBe(31);
  let current = 0;
  const result = await traverseTabs({
    limit: tabTraversalLimit(30),
    dispatchTab: async () => { current++; },
    focusedStop: async () => ({
      identity: `control-${current}`,
      label: `Control ${current}`,
      isTarget: current === 30,
    }),
  });
  expect(result).toMatchObject({ reached: true, tabs: 30 });
});

test("stops at the hard cap when a target is unreachable", async () => {
  expect(tabTraversalLimit(TAB_TRAVERSAL_HARD_CAP + 100)).toBe(TAB_TRAVERSAL_HARD_CAP);
  let current = 0;
  const result = await traverseTabs({
    limit: 3,
    dispatchTab: async () => { current++; },
    focusedStop: async () => ({ identity: `control-${current}`, label: `Control ${current}`, isTarget: false }),
  });
  expect(result).toMatchObject({ reached: false, reason: "derived bound", limit: 3 });
});

test("detects focus cycles and reports the visited focus stops", async () => {
  const stops = ["menu", "search", "menu"];
  let current = 0;
  const result = await traverseTabs({
    limit: 10,
    dispatchTab: async () => {},
    focusedStop: async () => {
      const identity = stops[current++]!;
      return { identity, label: identity === "menu" ? "<button> Menu" : "<input> Search", isTarget: false };
    },
  });
  expect(result).toMatchObject({ reached: false, reason: "focus cycle", limit: 10 });
  if (result.reached) throw new Error("expected an unreachable target");
  expect(tabTraversalDiagnostics(result.visited)).toBe(
    "1:<button> Menu [menu] -> 2:<input> Search [search] -> 3:<button> Menu [menu]",
  );
});
