// Generated from DESIGN.md by scripts/design-tokens.ts. Do not edit.
export const designTokens = {
  "name": "Rentemester",
  "colors": {
    "paper": "#F4F1EB",
    "paperRaised": "#FBF8F3",
    "ink": "#1B1A17",
    "inkMuted": "#4C4740",
    "accent": "#A6332A",
    "onAccent": "#F4F1EB",
    "danger": "#8F2A22",
    "success": "#2E5E4E",
    "warning": "#8A5A12",
    "info": "#2D5673",
    "accentSoft": "#E8D7D3",
    "dangerSoft": "#EED9D6",
    "successSoft": "#DCE8E1",
    "warningSoft": "#EEE3D1",
    "infoSoft": "#D9E4EB",
    "border": "#D8D2C6",
    "borderStrong": "#8F887D"
  },
  "typography": {
    "headlineFamily": "Source Serif 4",
    "bodyFamily": "IBM Plex Sans",
    "monoFamily": "IBM Plex Mono",
    "bodySize": "16px",
    "bodyLineHeight": "1.5",
    "monoFeatures": "tnum",
    "sizeXs": "12px",
    "sizeSm": "14px",
    "sizeMd": "16px",
    "sizeLg": "18px",
    "sizeXl": "24px",
    "size2xl": "32px"
  },
  "spacing": {
    "xxs": "4px",
    "xs": "8px",
    "sm": "12px",
    "md": "16px",
    "lg": "24px",
    "xl": "32px",
    "2xl": "48px"
  },
  "layout": {
    "maxWidth": "1440px",
    "sidebarWidth": "248px",
    "proseWidth": "68ch",
    "mobileBreakpoint": "640px",
    "navigationBreakpoint": "1024px"
  },
  "controls": {
    "height": "44px",
    "compactHeight": "36px",
    "focusWidth": "2px",
    "focusOffset": "3px"
  },
  "rounded": {
    "sm": "2px",
    "md": "4px",
    "lg": "8px"
  },
  "components": {
    "buttonPrimary": {
      "backgroundColor": "#1B1A17",
      "textColor": "#F4F1EB",
      "borderRadius": "4px",
      "paddingInline": "16px",
      "paddingBlock": "8px"
    },
    "buttonSecondary": {
      "backgroundColor": "#FBF8F3",
      "textColor": "#1B1A17",
      "borderColor": "#8F887D",
      "borderRadius": "4px"
    },
    "buttonDanger": {
      "backgroundColor": "#8F2A22",
      "textColor": "#F4F1EB",
      "borderRadius": "4px"
    },
    "tableRow": {
      "backgroundColor": "#F4F1EB",
      "textColor": "#1B1A17",
      "borderColor": "#D8D2C6"
    },
    "badgeStatusPaid": {
      "backgroundColor": "#DCE8E1",
      "textColor": "#1B1A17",
      "borderRadius": "2px"
    },
    "badgeStatusOverdue": {
      "backgroundColor": "#EED9D6",
      "textColor": "#1B1A17",
      "borderRadius": "2px"
    },
    "alertDanger": {
      "backgroundColor": "#EED9D6",
      "textColor": "#1B1A17",
      "borderColor": "#8F2A22"
    },
    "amountCell": {
      "textColor": "#1B1A17",
      "fontFamily": "IBM Plex Mono"
    },
    "buttonQuiet": {
      "backgroundColor": "transparent",
      "textColor": "#1B1A17"
    },
    "formControl": {
      "backgroundColor": "#FBF8F3",
      "textColor": "#1B1A17",
      "borderColor": "#8F887D",
      "borderRadius": "4px",
      "minHeight": "44px",
      "fontSize": "16px",
      "paddingInline": "12px",
      "paddingBlock": "8px"
    },
    "controlFocus": {
      "outlineColor": "#2D5673",
      "outlineWidth": "2px",
      "outlineOffset": "3px"
    },
    "fieldHelp": {
      "textColor": "#4C4740",
      "fontSize": "14px"
    },
    "fieldError": {
      "textColor": "#8F2A22",
      "fontSize": "14px"
    },
    "pageHeader": {
      "fontFamily": "Source Serif 4",
      "fontSize": "32px",
      "marginBottom": "24px"
    },
    "dialog": {
      "backgroundColor": "#FBF8F3",
      "textColor": "#1B1A17",
      "borderColor": "#8F887D",
      "borderRadius": "4px",
      "padding": "24px",
      "maxWidth": "640px",
      "viewportInset": "16px",
      "backdropColor": "rgb(27 26 23 / 40%)",
      "titleSize": "24px"
    },
    "navigation": {
      "sidebarWidth": "248px",
      "linkMinHeight": "44px"
    },
    "filterBar": {
      "backgroundColor": "#FBF8F3",
      "borderColor": "#D8D2C6",
      "borderRadius": "4px",
      "padding": "16px"
    },
    "table": {
      "textSize": "14px",
      "headerBackground": "#FBF8F3",
      "cellPadding": "12px",
      "separatorColor": "#D8D2C6"
    },
    "badgeStatusWarning": {
      "backgroundColor": "#EEE3D1",
      "textColor": "#1B1A17",
      "borderRadius": "2px"
    },
    "badgeStatusInfo": {
      "backgroundColor": "#D9E4EB",
      "textColor": "#1B1A17",
      "borderRadius": "2px"
    },
    "resultReceipt": {
      "backgroundColor": "#FBF8F3",
      "borderColor": "#D8D2C6",
      "borderRadius": "4px",
      "padding": "16px"
    },
    "chart": {
      "incomeColor": "#2E5E4E",
      "expenseColor": "#A6332A",
      "comparisonColor": "#2D5673",
      "legendSize": "14px",
      "axisSize": "12px"
    }
  }
} as const;
