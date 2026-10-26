import { describe, expect, test } from "bun:test";
import {
  horizontalOverflowFailure,
  horizontalOverflowSnapshotExpression,
} from "../../scripts/release/cockpit-evidence-overflow";

describe("Cockpit evidence overflow diagnostics", () => {
  test("preserves the strict document assertion and identifies browser-observed offenders", () => {
    expect(horizontalOverflowSnapshotExpression).toContain("document.documentElement.scrollWidth<=viewportWidth");
    const message = horizontalOverflowFailure("issue-650-normal-mobile", {
      viewportWidth: 390,
      documentScrollWidth: 612,
      offenders: [{ selector: "div.row-actions", tag: "div", classes: ["row-actions"], rect: { left: 16, right: 612, width: 596 }, scrollWidth: 596, clientWidth: 358 }],
    });
    expect(message).toContain("issue-650-normal-mobile has no horizontal overflow");
    expect(message).toContain('"selector":"div.row-actions"');
    expect(message).toContain('"tag":"div"');
    expect(message).toContain('"classes":["row-actions"]');
    expect(message).toContain('"rect":{"left":16,"right":612,"width":596}');
    expect(message).toContain('"scrollWidth":596');
  });
});
