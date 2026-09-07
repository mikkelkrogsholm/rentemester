import { expect, test } from "bun:test";
import { cdpKeyEvents } from "../../scripts/release/cockpit-evidence-key-events";

test("builds Chrome-compatible Enter events with carriage-return text", () => {
  expect(cdpKeyEvents("Enter")).toEqual([
    {
      type: "rawKeyDown", key: "Enter", code: "Enter", modifiers: 0,
      windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13,
      text: "\r", unmodifiedText: "\r",
    },
    {
      type: "keyUp", key: "Enter", code: "Enter", modifiers: 0,
      windowsVirtualKeyCode: 13, nativeVirtualKeyCode: 13,
    },
  ]);
});

test("builds Chrome-compatible Space events with printable text", () => {
  expect(cdpKeyEvents("Space")).toEqual([
    {
      type: "rawKeyDown", key: " ", code: "Space", modifiers: 0,
      windowsVirtualKeyCode: 32, nativeVirtualKeyCode: 32,
      text: " ", unmodifiedText: " ",
    },
    {
      type: "keyUp", key: " ", code: "Space", modifiers: 0,
      windowsVirtualKeyCode: 32, nativeVirtualKeyCode: 32,
    },
  ]);
});

test("builds Tab events without text", () => {
  expect(cdpKeyEvents("Tab")).toEqual([
    {
      type: "rawKeyDown", key: "Tab", code: "Tab", modifiers: 0,
      windowsVirtualKeyCode: 9, nativeVirtualKeyCode: 9,
    },
    {
      type: "keyUp", key: "Tab", code: "Tab", modifiers: 0,
      windowsVirtualKeyCode: 9, nativeVirtualKeyCode: 9,
    },
  ]);
});

test("builds Shift+Tab events with Shift held and no text", () => {
  expect(cdpKeyEvents("Shift+Tab")).toEqual([
    {
      type: "rawKeyDown", key: "Tab", code: "Tab", modifiers: 8,
      windowsVirtualKeyCode: 9, nativeVirtualKeyCode: 9,
    },
    {
      type: "keyUp", key: "Tab", code: "Tab", modifiers: 8,
      windowsVirtualKeyCode: 9, nativeVirtualKeyCode: 9,
    },
  ]);
});
