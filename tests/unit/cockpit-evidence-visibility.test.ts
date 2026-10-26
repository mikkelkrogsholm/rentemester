import { expect, test } from "bun:test";
import { sightedUserVisibleSource } from "../../scripts/release/cockpit-evidence-visibility";

type FakeNode = {
  tagName?: string;
  open?: boolean;
  parentElement: FakeNode | null;
  classList: { contains(name: string): boolean };
  querySelector(selector: string): FakeNode | null;
  contains(node: FakeNode): boolean;
  style?: Partial<CSSStyleDeclaration>;
};

function node(tagName?: string, parentElement: FakeNode | null = null): FakeNode {
  const value: FakeNode = {
    tagName,
    parentElement,
    classList: { contains: () => false },
    querySelector: () => null,
    contains(candidate) {
      for (let current: FakeNode | null = candidate; current; current = current.parentElement)
        if (current === value) return true;
      return false;
    },
  };
  return value;
}

function predicate() {
  return new Function(
    "getComputedStyle",
    "document",
    `return (${sightedUserVisibleSource})`,
  )(
    (element: FakeNode) => ({
      display: element.style?.display ?? "block",
      visibility: element.style?.visibility ?? "visible",
      contentVisibility: element.style?.contentVisibility ?? "visible",
    }),
    {
      createRange: () => ({
        selectNodeContents() {},
        getClientRects: () => [{ width: 1, height: 1 }],
      }),
    },
  ) as (value: FakeNode) => boolean;
}

test("sighted browser visibility respects native details disclosure", () => {
  const isVisible = predicate();
  const body = node("BODY");
  const details = node("DETAILS", body);
  const summary = node("SUMMARY", details);
  const summaryText = node(undefined, summary);
  const content = node("P", details);
  const contentText = node(undefined, content);
  details.querySelector = (selector) => selector === ":scope > summary" ? summary : null;

  expect(isVisible(contentText)).toBe(false);
  expect(isVisible(summaryText)).toBe(true);

  details.open = true;
  expect(isVisible(contentText)).toBe(true);

  const nested = node("DETAILS", details);
  const nestedSummary = node("SUMMARY", nested);
  const nestedContent = node("P", nested);
  const nestedContentText = node(undefined, nestedContent);
  nested.querySelector = (selector) => selector === ":scope > summary" ? nestedSummary : null;

  expect(isVisible(nestedContentText)).toBe(false);
  nested.open = true;
  expect(isVisible(nestedContentText)).toBe(true);

  details.open = false;
  expect(isVisible(nestedContentText)).toBe(false);
});

test("sighted browser visibility keeps hidden and sr-only text excluded", () => {
  const isVisible = predicate();
  const body = node("BODY");
  const hidden = node("SPAN", body);
  hidden.style = { display: "none" };
  const hiddenText = node(undefined, hidden);
  const srOnly = node("SPAN", body);
  srOnly.classList = { contains: (name) => name === "sr-only" };
  const srOnlyText = node(undefined, srOnly);

  expect(isVisible(hiddenText)).toBe(false);
  expect(isVisible(srOnlyText)).toBe(false);
});
