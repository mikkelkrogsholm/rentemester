/** Chrome DevTools Protocol payloads for the keyboard evidence controls. */
export type EvidenceKey = "Tab" | "Shift+Tab" | "Enter" | "Space";

export type CdpKeyEvent = {
  type: "rawKeyDown" | "keyDown" | "keyUp";
  key: string;
  code: string;
  modifiers: number;
  windowsVirtualKeyCode: number;
  nativeVirtualKeyCode: number;
  text?: string;
  unmodifiedText?: string;
};

type KeyDefinition = Omit<CdpKeyEvent, "type" | "text" | "unmodifiedText"> & {
  text?: string;
};

const KEY_DEFINITIONS: Readonly<Record<EvidenceKey, KeyDefinition>> = {
  Tab: {
    key: "Tab",
    code: "Tab",
    modifiers: 0,
    windowsVirtualKeyCode: 9,
    nativeVirtualKeyCode: 9,
  },
  "Shift+Tab": {
    key: "Tab",
    code: "Tab",
    modifiers: 8,
    windowsVirtualKeyCode: 9,
    nativeVirtualKeyCode: 9,
  },
  Enter: {
    key: "Enter",
    code: "Enter",
    modifiers: 0,
    windowsVirtualKeyCode: 13,
    nativeVirtualKeyCode: 13,
    text: "\r",
  },
  Space: {
    key: " ",
    code: "Space",
    modifiers: 0,
    windowsVirtualKeyCode: 32,
    nativeVirtualKeyCode: 32,
    text: " ",
  },
};

/**
 * Builds the native key pair Chrome needs to perform browser default actions.
 * Textual keys use keyDown; non-textual keys use rawKeyDown.
 */
export function cdpKeyEvents(key: EvidenceKey): readonly [CdpKeyEvent, CdpKeyEvent] {
  const { text, ...definition } = KEY_DEFINITIONS[key];
  const keyDown: CdpKeyEvent = {
    type: text === undefined ? "rawKeyDown" : "keyDown",
    ...definition,
  };
  if (text !== undefined) {
    keyDown.text = text;
    keyDown.unmodifiedText = text;
  }
  return [
    keyDown,
    { type: "keyUp", ...definition },
  ];
}
