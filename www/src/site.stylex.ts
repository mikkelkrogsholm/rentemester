import * as stylex from "@stylexjs/stylex";

// The static compiler supplies this literal from DESIGN.md before evaluation.
declare const marketing: { colors: Record<string, string>; alpha: Record<string, Record<string, string>>; typography: Record<string, string>; layout: Record<string, string> };

// Marketing identity is intentionally dark. Styles are compiled for static Astro markup.
const base = stylex.create({
  "element": {
    "backgroundColor": {
      "default": null,
      "::selection": marketing.colors.brassGold
    },
    "color": {
      "default": null,
      "::selection": marketing.colors.onPrimaryContainer
    },
    "boxSizing": "border-box",
    "borderTopWidth": 0,
    "borderRightWidth": 0,
    "borderBottomWidth": 0,
    "borderLeftWidth": 0,
    "borderStyle": "solid",
    "borderColor": marketing.colors.elementBorder
  },
  "html": {
    "colorScheme": "dark",
    "backgroundColor": marketing.colors.inkBlack,
    "color": marketing.colors.onSurface,
    "fontFamily": marketing.typography.baseBody,
    "WebkitFontSmoothing": "antialiased",
    "textRendering": "optimizeLegibility",
    "overflowX": "hidden",
    "lineHeight": 1.5,
    "WebkitTextSizeAdjust": "100%",
    "tabSize": 4,
    "WebkitTapHighlightColor": "transparent"
  },
  "body": {
    "margin": 0,
    "lineHeight": "inherit",
    "backgroundColor": marketing.colors.inkBlack,
    "color": marketing.colors.onSurface,
    "overflowX": "hidden",
    "width": "100%"
  },
  "h1": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0,
    "fontFamily": marketing.typography.baseHeadline
  },
  "h2": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0,
    "fontFamily": marketing.typography.baseHeadline
  },
  "h3": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0,
    "fontFamily": marketing.typography.baseHeadline
  },
  "h4": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0,
    "fontFamily": marketing.typography.baseHeadline
  },
  "h5": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0
  },
  "h6": {
    "fontSize": "inherit",
    "fontWeight": "inherit",
    "margin": 0
  },
  "a": {
    "color": "inherit",
    "textDecoration": "none",
    "outlineColor": {
      "default": null,
      ":focus-visible": marketing.colors.primary
    },
    "outlineStyle": {
      "default": null,
      ":focus-visible": "solid"
    },
    "outlineWidth": {
      "default": null,
      ":focus-visible": 2
    },
    "outlineOffset": {
      "default": null,
      ":focus-visible": 2
    }
  },
  "strong": {
    "fontWeight": "bolder"
  },
  "b": {
    "fontWeight": "bolder"
  },
  "code": {
    "fontFamily": marketing.typography.baseMono,
    "fontSize": "1em"
  },
  "pre": {
    "fontFamily": marketing.typography.baseMono,
    "fontSize": "1em",
    "margin": 0
  },
  "p": {
    "margin": 0
  },
  "blockquote": {
    "margin": 0
  },
  "dl": {
    "margin": 0
  },
  "dd": {
    "margin": 0
  },
  "figure": {
    "margin": 0
  },
  "hr": {
    "margin": 0,
    "height": 0,
    "color": "inherit",
    "borderTopWidth": 1
  },
  "table": {
    "textIndent": 0,
    "borderColor": "inherit",
    "borderCollapse": "collapse"
  },
  "fieldset": {
    "margin": 0,
    "padding": 0
  },
  "legend": {
    "padding": 0
  },
  "ol": {
    "listStyleType": "none",
    "margin": 0,
    "padding": 0
  },
  "ul": {
    "listStyleType": "none",
    "margin": 0,
    "padding": 0
  },
  "menu": {
    "listStyleType": "none",
    "margin": 0,
    "padding": 0
  },
  "summary": {
    "display": "list-item"
  },
  "svg": {
    "display": "block",
    "verticalAlign": "middle"
  },
  "img": {
    "display": "block",
    "verticalAlign": "middle",
    "maxWidth": "100%",
    "height": "auto"
  },
  "input": {
    "fontFamily": "inherit",
    "fontSize": "100%",
    "fontWeight": "inherit",
    "lineHeight": "inherit",
    "letterSpacing": "inherit",
    "color": "inherit",
    "margin": 0,
    "padding": 0
  },
  "button": {
    "fontFamily": "inherit",
    "fontSize": "100%",
    "fontWeight": "inherit",
    "lineHeight": "inherit",
    "letterSpacing": "inherit",
    "color": "inherit",
    "margin": 0,
    "padding": 0,
    "textTransform": "none",
    "WebkitAppearance": "button",
    "backgroundColor": "transparent",
    "backgroundImage": "none",
    "cursor": "pointer",
    "outlineColor": {
      "default": null,
      ":focus-visible": marketing.colors.primary
    },
    "outlineStyle": {
      "default": null,
      ":focus-visible": "solid"
    },
    "outlineWidth": {
      "default": null,
      ":focus-visible": 2
    },
    "outlineOffset": {
      "default": null,
      ":focus-visible": 2
    }
  },
  "select": {
    "fontFamily": "inherit",
    "fontSize": "100%",
    "fontWeight": "inherit",
    "lineHeight": "inherit",
    "letterSpacing": "inherit",
    "color": "inherit",
    "margin": 0,
    "padding": 0,
    "textTransform": "none"
  },
  "textarea": {
    "fontFamily": "inherit",
    "fontSize": "100%",
    "fontWeight": "inherit",
    "lineHeight": "inherit",
    "letterSpacing": "inherit",
    "color": "inherit",
    "margin": 0,
    "padding": 0,
    "resize": "vertical"
  }
});

const styles = stylex.create({
  "components_Footer_footer1": {
    "marginTop": "6rem",
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.3"],
    "borderRightColor": marketing.alpha.outlineVariant["0.3"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.3"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.3"],
    "backgroundColor": marketing.colors.deepNavy
  },
  "components_Footer_div2": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "display": "grid",
    "maxWidth": marketing.layout.maxWidth,
    "gap": "3rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "4rem",
    "paddingBottom": "4rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(4, minmax(0, 1fr))"
    }
  },
  "components_Footer_div3": {
    "gridColumn": {
      "default": null,
      "@media (min-width: 768px)": "span 2 / span 2"
    }
  },
  "components_Footer_div4": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.primary
  },
  "components_Footer_p5": {
    "maxWidth": "28rem",
    "fontSize": "15px",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "components_Footer_a6": {
    "marginTop": "1.5rem",
    "display": "inline-flex",
    "alignItems": "center",
    "gap": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": {
      "default": marketing.colors.primary,
      ":hover": marketing.colors.cyberBlue
    },
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "components_Footer_svg7": {
    "fill": "currentColor"
  },
  "components_Footer_path8": {},
  "components_Footer_div9": {},
  "components_Footer_div10": {
    "marginBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.outline
  },
  "components_Footer_ul11": {
    "fontSize": "15px"
  },
  "components_Footer_li12": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "0.5rem"
    }
  },
  "components_Footer_a13": {
    "color": {
      "default": marketing.colors.onSurfaceVariant,
      ":hover": marketing.colors.primary
    }
  },
  "components_Footer_div14": {
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"]
  },
  "components_Footer_div15": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "display": "flex",
    "maxWidth": marketing.layout.maxWidth,
    "flexDirection": {
      "default": "column",
      "@media (min-width: 768px)": "row"
    },
    "justifyContent": "space-between",
    "gap": "1rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "1.5rem",
    "paddingBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": marketing.colors.outline
  },
  "components_Footer_div16": {
    "display": "flex",
    "flexWrap": "wrap",
    "gap": "1.5rem"
  },
  "components_Footer_a17": {
    "color": {
      "default": null,
      ":hover": marketing.colors.primary
    }
  },
  "components_PageHero_section1": {
    "position": "relative",
    "overflow": "hidden",
    "borderBottomWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"]
  },
  "components_PageHero_div2": {
    "position": "absolute",
    "top": "0px",
    "right": "0px",
    "bottom": "0px",
    "left": "0px",
    "zIndex": "-10"
  },
  "components_PageHero_div3": {
    "position": "absolute",
    "left": "50%",
    "top": "-30%",
    "aspectRatio": "1 / 1",
    "width": "900px",
    "transform": "translate(-50%, 0) rotate(0) skewX(0) skewY(0) scaleX(1) scaleY(1)",
    "borderRadius": "9999px",
    "backgroundColor": marketing.alpha.primary["0.06"],
    "filter": "blur(140px)"
  },
  "components_PageHero_div4": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    },
    "paddingTop": {
      "default": "4rem",
      "@media (min-width: 768px)": "6rem"
    }
  },
  "components_PageHero_div5": {
    "maxWidth": "48rem"
  },
  "components_PageHero_div6": {
    "marginBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.cyberBlue
  },
  "components_PageHero_h17": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 640px)": "3rem",
      "@media (min-width: 768px)": "3.75rem"
    },
    "lineHeight": {
      "default": "1.05",
      "@media (min-width: 640px)": "1",
      "@media (min-width: 768px)": "1"
    },
    "fontWeight": "600",
    "letterSpacing": "-0.02em",
    "color": marketing.colors.onSurface
  },
  "components_PageHero_p8": {
    "maxWidth": "42rem",
    "fontSize": {
      "default": "1.125rem",
      "@media (min-width: 768px)": "1.25rem"
    },
    "lineHeight": {
      "default": "1.625",
      "@media (min-width: 768px)": "1.75rem"
    },
    "color": marketing.colors.onSurfaceVariant
  },
  "components_KnowledgeSectionLinks_aside1": {
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.alpha.surfaceContainerLow["0.4"]
  },
  "components_KnowledgeSectionLinks_div2": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "2.5rem",
      "@media (min-width: 768px)": "3rem"
    },
    "paddingBottom": {
      "default": "2.5rem",
      "@media (min-width: 768px)": "3rem"
    }
  },
  "components_KnowledgeSectionLinks_div3": {
    "display": "grid",
    "gap": "0.75rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))"
    }
  },
  "components_KnowledgeSectionLinks_a4": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingLeft": "1rem",
    "paddingRight": "1rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontSize": "15px",
    "color": {
      "default": marketing.colors.onSurfaceVariant,
      ":hover": marketing.colors.primary
    },
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "components_KnowledgeSectionLinks_a5": {
    "marginTop": "1.5rem",
    "display": "inline-flex",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": {
      "default": marketing.colors.cyberBlue,
      ":hover": marketing.colors.primary
    }
  },
  "components_Header_header1": {
    "position": "fixed",
    "left": "0px",
    "right": "0px",
    "top": "0px",
    "zIndex": "50",
    "borderBottomWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.alpha.inkBlack["0.85"],
    "WebkitBackdropFilter": "blur(12px)",
    "backdropFilter": "blur(12px)"
  },
  "components_Header_div2": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "display": "flex",
    "height": "4rem",
    "maxWidth": {
      "default": "100vw",
      "@media (min-width: 768px)": "1280px"
    },
    "alignItems": "center",
    "justifyContent": "space-between",
    "gap": "0.75rem",
    "paddingLeft": {
      "default": "1rem",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "calc(1.5rem + env(safe-area-inset-right, 0px))",
      "@media (min-width: 768px)": "64px"
    },
    "boxSizing": "border-box",
    "width": "100%",
    "minWidth": 0,
    "overflow": "visible"
  },
  "components_Header_a3": {
    "minWidth": "0px",
    "overflow": "hidden",
    "textOverflow": "ellipsis",
    "whiteSpace": "nowrap",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "22px",
      "@media (min-width: 640px)": "1.5rem",
      "@media (min-width: 768px)": "28px"
    },
    "letterSpacing": "-0.025em",
    "color": marketing.colors.primary,
    "lineHeight": {
      "default": null,
      "@media (min-width: 640px)": "2rem"
    }
  },
  "components_Header_nav4": {
    "alignItems": "center",
    "gap": "2rem",
    "display": {
      "default": "none",
      "@media (min-width: 768px)": "flex"
    }
  },
  "components_Header_a5": {
    "borderBottomWidth": "2px",
    "borderTopColor": marketing.colors.primary,
    "borderRightColor": marketing.colors.primary,
    "borderBottomColor": marketing.colors.primary,
    "borderLeftColor": marketing.colors.primary,
    "paddingBottom": "0.25rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": marketing.colors.primary,
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "components_Header_a6": {
    "paddingBottom": "0.25rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": {
      "default": marketing.colors.onSurfaceVariant,
      ":hover": marketing.colors.primary
    },
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "components_Header_div7": {
    "display": "flex",
    "flexShrink": "0",
    "alignItems": "center",
    "gap": {
      "default": "0.5rem",
      "@media (min-width: 640px)": "0.75rem"
    },
    "minWidth": {
      "default": "2.5rem",
      "@media (min-width: 768px)": null
    }
  },
  "components_Header_a8": {
    "display": {
      "default": "none",
      "@media (min-width: 640px)": "inline-flex"
    },
    "alignItems": "center",
    "gap": "0.5rem",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingTop": "0.5rem",
    "paddingBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "600",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": marketing.colors.inkBlack,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "filter": {
      "default": null,
      ":hover": "brightness(1.1)"
    },
    "transform": {
      "default": null,
      ":active": "translate(0, 0) rotate(0) skewX(0) skewY(0) scaleX(.95) scaleY(.95)"
    }
  },
  "components_Header_button9": {
    "height": "2.5rem",
    "width": "2.5rem",
    "flexShrink": "0",
    "alignItems": "center",
    "justifyContent": "center",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.4"],
    "borderRightColor": marketing.alpha.outlineVariant["0.4"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.4"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.4"],
    "color": marketing.colors.onSurface,
    "display": {
      "default": "inline-flex",
      "@media (min-width: 768px)": "none"
    },
    "marginRight": {
      "default": "0.25rem",
      "@media (min-width: 768px)": 0
    }
  },
  "components_Header_svg10": {
    "fill": "none",
    "stroke": "currentColor",
    "strokeWidth": "2"
  },
  "components_Header_line11": {},
  "components_Header_nav12": {
    "display": {
      "default": "block",
      ":is([hidden])": "none",
      "@media (min-width: 768px)": "none"
    },
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.colors.inkBlack
  },
  "components_Header_ul13": {
    "display": "flex",
    "flexDirection": "column",
    "gap": "0.25rem",
    "paddingLeft": "16px",
    "paddingRight": "16px",
    "paddingTop": "1rem",
    "paddingBottom": "1rem"
  },
  "components_Header_li14": {},
  "components_Header_a15": {
    "display": "block",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": {
      "default": marketing.colors.onSurfaceVariant,
      ":hover": marketing.colors.primary
    }
  },
  "components_Header_li16": {
    "marginTop": "0.5rem"
  },
  "components_Header_a17": {
    "display": "block",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.1em",
    "color": marketing.colors.primary
  },
  "components_SeoEvidence_aside1": {
    "marginTop": "3rem",
    "marginBottom": "3rem",
    "borderTopWidth": "1px",
    "borderBottomWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "paddingTop": "1.75rem",
    "paddingBottom": "1.75rem"
  },
  "components_SeoEvidence_p2": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.12em",
    "color": marketing.colors.outline
  },
  "components_SeoEvidence_a3": {
    "color": {
      "default": marketing.colors.cyberBlue,
      ":hover": marketing.colors.cyberBlue
    },
    "borderBottomWidth": "1px",
    "borderBottomStyle": "dotted",
    "borderBottomColor": {
      "default": `${marketing.alpha.cyberBlue["0.4"]}`,
      ":hover": marketing.colors.cyberBlue
    }
  },
  "components_SeoEvidence_time4": {},
  "components_SeoEvidence_div5": {
    "display": "grid",
    "gap": "2rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "0.75fr 1fr"
    }
  },
  "components_SeoEvidence_div6": {},
  "components_SeoEvidence_div7": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.18em",
    "color": marketing.colors.cyberBlue
  },
  "components_SeoEvidence_dl8": {
    "fontSize": "15px",
    "color": marketing.colors.onSurfaceVariant
  },
  "components_SeoEvidence_div9": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "0.5rem"
    }
  },
  "components_SeoEvidence_dt10": {
    "display": "inline",
    "color": marketing.colors.outline
  },
  "components_SeoEvidence_dd11": {
    "display": "inline",
    "color": marketing.colors.onSurface
  },
  "components_SeoEvidence_span12": {},
  "components_SeoEvidence_div13": {
    "display": "grid",
    "gap": "1.75rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))"
    }
  },
  "components_SeoEvidence_div14": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.18em",
    "color": marketing.colors.primary
  },
  "components_SeoEvidence_ul15": {},
  "components_SeoEvidence_li16": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "0.5rem"
    }
  },
  "components_SeoEvidence_a17": {
    "color": marketing.colors.cyberBlue,
    "borderBottomWidth": "1px",
    "borderBottomStyle": "dotted",
    "borderBottomColor": {
      "default": `${marketing.alpha.cyberBlue["0.4"]}`,
      ":hover": marketing.colors.cyberBlue
    }
  },
  "layouts_BaseLayout_html1": {},
  "layouts_BaseLayout_body2": {
    "minHeight": "100vh",
    "backgroundColor": marketing.colors.inkBlack,
    "color": marketing.colors.onSurface,
    "WebkitFontSmoothing": "antialiased",
    "MozOsxFontSmoothing": "grayscale"
  },
  "layouts_BaseLayout_a3": {
    "position": {
      "default": "absolute",
      ":focus": "fixed"
    },
    "width": {
      "default": "1px",
      ":focus": "auto"
    },
    "height": {
      "default": "1px",
      ":focus": "auto"
    },
    "paddingTop": {
      "default": "0",
      ":focus": "0.5rem"
    },
    "paddingRight": {
      "default": "0",
      ":focus": "0.75rem"
    },
    "paddingBottom": {
      "default": "0",
      ":focus": "0.5rem"
    },
    "paddingLeft": {
      "default": "0",
      ":focus": "0.75rem"
    },
    "marginTop": {
      "default": "-1px",
      ":focus": "0"
    },
    "marginRight": {
      "default": "-1px",
      ":focus": "0"
    },
    "marginBottom": {
      "default": "-1px",
      ":focus": "0"
    },
    "marginLeft": {
      "default": "-1px",
      ":focus": "0"
    },
    "overflow": {
      "default": "hidden",
      ":focus": "visible"
    },
    "clip": {
      "default": "rect(0, 0, 0, 0)",
      ":focus": "auto"
    },
    "whiteSpace": {
      "default": "nowrap",
      ":focus": "normal"
    },
    "borderTopWidth": "0",
    "borderRightWidth": "0",
    "borderBottomWidth": "0",
    "borderLeftWidth": "0",
    "left": {
      "default": null,
      ":focus": "0.5rem"
    },
    "top": {
      "default": null,
      ":focus": "0.5rem"
    },
    "zIndex": {
      "default": null,
      ":focus": "50"
    },
    "backgroundColor": {
      "default": null,
      ":focus": marketing.colors.brassGold
    },
    "fontFamily": {
      "default": null,
      ":focus": marketing.typography.utilityMono
    },
    "fontSize": {
      "default": null,
      ":focus": "0.75rem"
    },
    "lineHeight": {
      "default": null,
      ":focus": "1rem"
    },
    "color": {
      "default": null,
      ":focus": marketing.colors.inkBlack
    }
  },
  "layouts_BaseLayout_main4": {
    "paddingTop": "4rem"
  },
  "layouts_BaseLayout_nav5": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "marginBottom": "-0.75rem",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "1.25rem"
  },
  "layouts_BaseLayout_ol6": {
    "display": "flex",
    "flexWrap": "wrap",
    "alignItems": "center",
    "gap": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.12em",
    "color": marketing.colors.outline
  },
  "layouts_BaseLayout_li7": {
    "display": "flex",
    "alignItems": "center",
    "gap": "0.5rem"
  },
  "layouts_BaseLayout_span8": {},
  "layouts_BaseLayout_span9": {
    "color": marketing.colors.onSurfaceVariant
  },
  "layouts_BaseLayout_div10": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "marginBottom": "-1.5rem",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "1.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.12em",
    "color": marketing.colors.outline
  },
  "layouts_BaseLayout_span11": {},
  "layouts_BaseLayout_span12": {},
  "layouts_BaseLayout_time13": {},
  "layouts_BaseLayout_time14": {},
  "pages_viden_regnskab_skatteopgoerelse_article1": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px",
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    }
  },
  "pages_viden_regnskab_skatteopgoerelse_p2": {
    "marginBottom": "1.25rem",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_viden_regnskab_skatteopgoerelse_h23": {
    "color": marketing.colors.primary,
    "fontSize": "32px",
    "lineHeight": "40px",
    "marginTop": "3rem",
    "marginBottom": "1rem",
    "fontWeight": "500"
  },
  "pages_viden_saadanBogfoererDu_leverandoerfaktura_ul1": {
    "marginBottom": "1.25rem",
    "paddingLeft": "1.5rem",
    "listStyle": "disc"
  },
  "pages_viden_saadanBogfoererDu_leverandoerfaktura_li2": {
    "marginBottom": "0.5rem",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_viden_saadanBogfoererDu_udlaeg_a1": {
    "color": marketing.colors.cyberBlue,
    "borderBottomWidth": "1px",
    "borderBottomStyle": "dotted",
    "borderBottomColor": `${marketing.alpha.cyberBlue["0.4"]}`
  },
  "pages_viden_saadanBogfoererDu_udlaeg_pre2": {
    "backgroundColor": marketing.colors.deepNavy,
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": `${marketing.alpha.brassGold["0.15"]}`,
    "borderRightColor": `${marketing.alpha.brassGold["0.15"]}`,
    "borderBottomColor": `${marketing.alpha.brassGold["0.15"]}`,
    "borderLeftColor": `${marketing.alpha.brassGold["0.15"]}`,
    "paddingTop": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingBottom": "1.25rem",
    "paddingLeft": "1.25rem",
    "overflowX": "auto",
    "marginTop": "1.5rem",
    "marginRight": "0",
    "marginBottom": "1.5rem",
    "marginLeft": "0",
    "borderRadius": "4px"
  },
  "pages_viden_saadanBogfoererDu_udlaeg_code3": {
    "backgroundColor": "transparent",
    "color": marketing.colors.onSurface,
    "paddingTop": "0",
    "paddingRight": "0",
    "paddingBottom": "0",
    "paddingLeft": "0",
    "borderRadius": "2px",
    "fontSize": "0.92em",
    "backgroundImage": "none"
  },
  "pages_viden_fakturering_eFakturaOffentlig_h31": {
    "color": marketing.colors.onSurface,
    "fontSize": "22px",
    "lineHeight": "30px",
    "marginTop": "2rem",
    "marginBottom": "0.75rem",
    "fontWeight": "500"
  },
  "pages_viden_index_section1": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    }
  },
  "pages_viden_index_div2": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px",
    "marginBottom": "2.5rem",
    "maxWidth": "48rem"
  },
  "pages_viden_index_div3": {
    "display": "grid",
    "gap": marketing.layout.gutter,
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(2, minmax(0, 1fr))",
      "@media (min-width: 1024px)": "repeat(3, minmax(0, 1fr))"
    }
  },
  "pages_viden_index_article4": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "1.75rem",
    "paddingRight": "1.75rem",
    "paddingBottom": "1.75rem",
    "paddingLeft": "1.75rem"
  },
  "pages_viden_index_h25": {
    "marginBottom": "1rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.primary
  },
  "pages_viden_index_ul6": {},
  "pages_viden_index_li7": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "0.75rem"
    }
  },
  "pages_viden_index_a8": {
    "color": {
      "default": marketing.colors.cyberBlue,
      ":hover": marketing.colors.primary
    }
  },
  "pages_viden_frister_moms_strong1": {
    "color": marketing.colors.onSurface,
    "fontWeight": "600"
  },
  "pages_bogfoeringsprogram_ol1": {},
  "pages_privacyPolicy_article1": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px",
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "6rem",
    "paddingBottom": "6rem"
  },
  "pages_privacyPolicy_h12": {},
  "pages_sikkerhed_code1": {
    "backgroundColor": marketing.colors.surfaceContainer,
    "color": marketing.colors.primary,
    "paddingTop": "2px",
    "paddingRight": "6px",
    "paddingBottom": "2px",
    "paddingLeft": "6px",
    "borderRadius": "2px",
    "fontSize": "0.92em"
  },
  "pages_404_section1": {
    "display": "flex",
    "minHeight": "60vh",
    "alignItems": "center"
  },
  "pages_404_div2": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": "5rem",
    "paddingBottom": "5rem",
    "textAlign": "center"
  },
  "pages_404_h13": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "3rem",
      "@media (min-width: 768px)": "4.5rem"
    },
    "lineHeight": {
      "default": "1",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.onSurface
  },
  "pages_404_p4": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "marginBottom": "2.5rem",
    "maxWidth": "36rem",
    "fontSize": "1.125rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_404_div5": {
    "display": "flex",
    "flexDirection": {
      "default": "column",
      "@media (min-width: 640px)": "row"
    },
    "justifyContent": "center",
    "gap": "0.75rem"
  },
  "pages_404_a6": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "2rem",
    "paddingRight": "2rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "700",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.inkBlack,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "filter": {
      "default": null,
      ":hover": "brightness(1.1)"
    }
  },
  "pages_404_a7": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.cyberBlue["0.6"],
    "borderRightColor": marketing.alpha.cyberBlue["0.6"],
    "borderBottomColor": marketing.alpha.cyberBlue["0.6"],
    "borderLeftColor": marketing.alpha.cyberBlue["0.6"],
    "paddingLeft": "2rem",
    "paddingRight": "2rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.cyberBlue,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "backgroundColor": {
      "default": null,
      ":hover": marketing.alpha.cyberBlue["0.05"]
    }
  },
  "pages_index_section1": {
    "position": "relative",
    "overflow": "hidden"
  },
  "pages_index_div2": {
    "position": "absolute",
    "left": "50%",
    "top": "-20%",
    "aspectRatio": "1 / 1",
    "width": "1200px",
    "maxWidth": "150vw",
    "transform": "translate(-50%, 0) rotate(0) skewX(0) skewY(0) scaleX(1) scaleY(1)",
    "borderRadius": "9999px",
    "backgroundColor": marketing.alpha.primary["0.07"],
    "filter": "blur(140px)"
  },
  "pages_index_div3": {
    "position": "absolute",
    "right": "-10%",
    "top": "40%",
    "aspectRatio": "1 / 1",
    "width": "500px",
    "borderRadius": "9999px",
    "backgroundColor": marketing.alpha.cyberBlue["0.05"],
    "filter": "blur(120px)"
  },
  "pages_index_div4": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    }
  },
  "pages_index_div5": {
    "marginBottom": "2rem",
    "display": "inline-flex",
    "alignItems": "center",
    "gap": "0.75rem"
  },
  "pages_index_span6": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.cyberBlue["0.3"],
    "borderRightColor": marketing.alpha.cyberBlue["0.3"],
    "borderBottomColor": marketing.alpha.cyberBlue["0.3"],
    "borderLeftColor": marketing.alpha.cyberBlue["0.3"],
    "backgroundColor": marketing.alpha.cyberBlue["0.1"],
    "paddingLeft": "0.75rem",
    "paddingRight": "0.75rem",
    "paddingTop": "0.375rem",
    "paddingBottom": "0.375rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.cyberBlue
  },
  "pages_index_span7": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_h18": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "44px",
      "@media (min-width: 640px)": "56px",
      "@media (min-width: 768px)": "72px"
    },
    "fontWeight": "600",
    "lineHeight": "1.05",
    "letterSpacing": "-0.02em",
    "color": marketing.colors.onSurface
  },
  "pages_index_br9": {},
  "pages_index_p10": {
    "marginBottom": "1rem",
    "maxWidth": "42rem",
    "fontSize": {
      "default": "1.125rem",
      "@media (min-width: 768px)": "1.25rem"
    },
    "lineHeight": {
      "default": "1.625",
      "@media (min-width: 768px)": "1.75rem"
    },
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_strong11": {
    "fontWeight": "600",
    "color": marketing.colors.primary
  },
  "pages_index_p12": {
    "marginBottom": "1rem",
    "maxWidth": "42rem",
    "fontSize": {
      "default": "1rem",
      "@media (min-width: 768px)": "1.125rem"
    },
    "lineHeight": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    },
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_strong13": {
    "color": marketing.colors.onSurface
  },
  "pages_index_p14": {
    "marginBottom": "2.5rem",
    "maxWidth": "42rem",
    "fontSize": {
      "default": "1rem",
      "@media (min-width: 768px)": "1.125rem"
    },
    "lineHeight": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    },
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div15": {
    "display": "flex",
    "flexDirection": {
      "default": "column",
      "@media (min-width: 640px)": "row"
    },
    "gap": "0.75rem"
  },
  "pages_index_a16": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "2rem",
    "paddingRight": "2rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "700",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.inkBlack,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "boxShadow": {
      "default": null,
      ":hover": `0 0 24px ${marketing.alpha.brassGold["0.35"]}`
    },
    "transform": {
      "default": null,
      ":active": "translate(0, 0) rotate(0) skewX(0) skewY(0) scaleX(0.98) scaleY(0.98)"
    }
  },
  "pages_index_a17": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.cyberBlue["0.6"],
      ":hover": marketing.colors.cyberBlue
    },
    "borderRightColor": {
      "default": marketing.alpha.cyberBlue["0.6"],
      ":hover": marketing.colors.cyberBlue
    },
    "borderBottomColor": {
      "default": marketing.alpha.cyberBlue["0.6"],
      ":hover": marketing.colors.cyberBlue
    },
    "borderLeftColor": {
      "default": marketing.alpha.cyberBlue["0.6"],
      ":hover": marketing.colors.cyberBlue
    },
    "paddingLeft": "2rem",
    "paddingRight": "2rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.cyberBlue,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "backgroundColor": {
      "default": null,
      ":hover": marketing.alpha.cyberBlue["0.05"]
    }
  },
  "pages_index_div18": {
    "marginTop": "3rem",
    "maxWidth": "42rem"
  },
  "pages_index_div19": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.outline
  },
  "pages_index_div20": {
    "overflowX": "auto",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.colors.deepNavy,
    "paddingLeft": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "13px",
    "color": marketing.colors.parchmentText
  },
  "pages_index_span21": {
    "userSelect": "none",
    "color": marketing.colors.outline
  },
  "pages_index_span22": {
    "color": marketing.colors.cyberBlue
  },
  "pages_index_section23": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    }
  },
  "pages_index_section24": {
    "borderTopWidth": "1px",
    "borderBottomWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.alpha.deepNavy["0.4"]
  },
  "pages_index_div25": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "4rem",
      "@media (min-width: 768px)": "5rem"
    },
    "paddingBottom": {
      "default": "4rem",
      "@media (min-width: 768px)": "5rem"
    }
  },
  "pages_index_div26": {
    "display": "grid",
    "gap": {
      "default": "2.5rem",
      "@media (min-width: 768px)": "4rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(3, minmax(0, 1fr))"
    }
  },
  "pages_index_div27": {
    "gridColumn": {
      "default": null,
      "@media (min-width: 768px)": "span 1 / span 1"
    }
  },
  "pages_index_div28": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.cyberBlue
  },
  "pages_index_h229": {
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "1.875rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "2.5rem"
    },
    "color": marketing.colors.primary
  },
  "pages_index_div30": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px",
    "gridColumn": {
      "default": null,
      "@media (min-width: 768px)": "span 2 / span 2"
    }
  },
  "pages_index_section31": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    }
  },
  "pages_index_div32": {
    "display": "grid",
    "alignItems": "flex-start",
    "gap": {
      "default": "3rem",
      "@media (min-width: 1024px)": "5rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 1024px)": "0.9fr 1.3fr"
    }
  },
  "pages_index_div33": {
    "position": {
      "default": null,
      "@media (min-width: 1024px)": "sticky"
    },
    "top": {
      "default": null,
      "@media (min-width: 1024px)": "2rem"
    }
  },
  "pages_index_h234": {
    "marginBottom": "1.25rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.onSurface
  },
  "pages_index_p35": {
    "marginBottom": "1.5rem",
    "fontSize": "1.125rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_p36": {
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div37": {
    "position": {
      "default": "relative",
      "::before": "absolute"
    },
    "display": "grid",
    "gap": "18px",
    "content": {
      "default": null,
      "::before": "\"\""
    },
    "left": {
      "default": null,
      "::before": "13px"
    },
    "top": {
      "default": null,
      "::before": "14px"
    },
    "bottom": {
      "default": null,
      "::before": "14px"
    },
    "width": {
      "default": null,
      "::before": "2px"
    },
    "background": {
      "default": null,
      "::before": `linear-gradient(180deg, ${marketing.colors.cyberBlue} 0%, ${marketing.colors.brassGold} 45%, ${marketing.alpha.brassGold["0.15"]} 100%)`
    }
  },
  "pages_index_article38": {
    "position": "relative",
    "paddingLeft": "42px"
  },
  "pages_index_div39_live": {
    "position": "absolute",
    "left": "4px",
    "top": "24px",
    "width": "20px",
    "height": "20px",
    "borderRadius": "999px",
    "borderTopWidth": "2px",
    "borderRightWidth": "2px",
    "borderBottomWidth": "2px",
    "borderLeftWidth": "2px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.colors.cyberBlue,
    "borderRightColor": marketing.colors.cyberBlue,
    "borderBottomColor": marketing.colors.cyberBlue,
    "borderLeftColor": marketing.colors.cyberBlue,
    "boxShadow": `0 0 0 6px ${marketing.colors.surface}, 0 0 18px ${marketing.alpha.cyberBlue["0.45"]}`,
    "backgroundColor": marketing.colors.cyberBlue
  },
  "pages_index_div40_now": {
    "position": "absolute",
    "left": "4px",
    "top": "24px",
    "width": "20px",
    "height": "20px",
    "borderRadius": "999px",
    "borderTopWidth": "2px",
    "borderRightWidth": "2px",
    "borderBottomWidth": "2px",
    "borderLeftWidth": "2px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.colors.primary,
    "borderRightColor": marketing.colors.primary,
    "borderBottomColor": marketing.colors.primary,
    "borderLeftColor": marketing.colors.primary,
    "boxShadow": `0 0 0 6px ${marketing.colors.surface}, 0 0 22px ${marketing.alpha.primary["0.55"]}`,
    "backgroundColor": marketing.colors.primary
  },
  "pages_index_div41_next": {
    "position": "absolute",
    "left": "4px",
    "top": "24px",
    "width": "20px",
    "height": "20px",
    "borderRadius": "999px",
    "borderTopWidth": "2px",
    "borderRightWidth": "2px",
    "borderBottomWidth": "2px",
    "borderLeftWidth": "2px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.colors.primary,
    "borderRightColor": marketing.colors.primary,
    "borderBottomColor": marketing.colors.primary,
    "borderLeftColor": marketing.colors.primary,
    "boxShadow": `0 0 0 6px ${marketing.colors.surface}`,
    "backgroundColor": marketing.colors.inkBlack
  },
  "pages_index_div42_future": {
    "position": "absolute",
    "left": "4px",
    "top": "24px",
    "width": "20px",
    "height": "20px",
    "borderRadius": "999px",
    "borderTopWidth": "2px",
    "borderRightWidth": "2px",
    "borderBottomWidth": "2px",
    "borderLeftWidth": "2px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.colors.outlineVariant,
    "borderRightColor": marketing.colors.outlineVariant,
    "borderBottomColor": marketing.colors.outlineVariant,
    "borderLeftColor": marketing.colors.outlineVariant,
    "boxShadow": `0 0 0 6px ${marketing.colors.surface}`,
    "backgroundColor": marketing.colors.inkBlack
  },
  "pages_index_div43": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    },
    "paddingRight": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    },
    "paddingBottom": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    },
    "paddingLeft": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "1.75rem"
    }
  },
  "pages_index_div44": {
    "marginBottom": "0.75rem",
    "display": "flex",
    "alignItems": "center",
    "justifyContent": "space-between",
    "gap": "1rem"
  },
  "pages_index_span45": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.18em",
    "color": marketing.colors.primary
  },
  "pages_index_span46": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "10px",
    "textTransform": "uppercase",
    "letterSpacing": "0.16em",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_h347": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.onSurface
  },
  "pages_index_p48": {
    "fontSize": "15px",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div49": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "6rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "6rem"
    }
  },
  "pages_index_div50": {
    "display": "grid",
    "alignItems": "flex-start",
    "gap": {
      "default": "3rem",
      "@media (min-width: 1024px)": "5rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 1024px)": "1fr 1.15fr"
    }
  },
  "pages_index_h251": {
    "marginBottom": "1.25rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.primary
  },
  "pages_index_p52": {
    "marginBottom": "1.25rem",
    "fontSize": "1.125rem",
    "lineHeight": "1.625",
    "color": marketing.colors.parchmentText
  },
  "pages_index_div53": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.cyberBlue["0.25"],
    "borderRightColor": marketing.alpha.cyberBlue["0.25"],
    "borderBottomColor": marketing.alpha.cyberBlue["0.25"],
    "borderLeftColor": marketing.alpha.cyberBlue["0.25"],
    "backgroundColor": marketing.colors.inkBlack,
    "paddingTop": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "2rem"
    },
    "paddingRight": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "2rem"
    },
    "paddingBottom": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "2rem"
    },
    "paddingLeft": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "2rem"
    },
    "boxShadow": `0 0 40px ${marketing.alpha.cyberBlue["0.06"]}`
  },
  "pages_index_pre54": {
    "whiteSpace": "pre-wrap",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "13px",
    "lineHeight": "1.625",
    "color": marketing.colors.parchmentText
  },
  "pages_index_code55": {},
  "pages_index_div56": {
    "marginTop": "1.5rem",
    "display": "flex",
    "flexDirection": {
      "default": "column",
      "@media (min-width: 640px)": "row"
    },
    "gap": "0.75rem"
  },
  "pages_index_a57": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "700",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.inkBlack,
    "filter": {
      "default": null,
      ":hover": "brightness(1.1)"
    }
  },
  "pages_index_a58": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.cyberBlue["0.6"],
    "borderRightColor": marketing.alpha.cyberBlue["0.6"],
    "borderBottomColor": marketing.alpha.cyberBlue["0.6"],
    "borderLeftColor": marketing.alpha.cyberBlue["0.6"],
    "paddingLeft": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.cyberBlue,
    "backgroundColor": {
      "default": null,
      ":hover": marketing.alpha.cyberBlue["0.05"]
    }
  },
  "pages_index_div59": {
    "marginBottom": {
      "default": "3.5rem",
      "@media (min-width: 768px)": "5rem"
    },
    "maxWidth": "42rem"
  },
  "pages_index_h260": {
    "marginBottom": "1rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.onSurface
  },
  "pages_index_p61": {
    "fontSize": "1.125rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div62": {
    "display": "grid",
    "gap": marketing.layout.gutter,
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(3, minmax(0, 1fr))"
    }
  },
  "pages_index_article63": {
    "boxShadow": `inset 0 0 15px ${marketing.alpha.cyberBlue["0.05"]}`,
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingRight": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingBottom": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingLeft": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    }
  },
  "pages_index_div64": {
    "marginBottom": "2rem",
    "display": "flex",
    "alignItems": "flex-start",
    "justifyContent": "space-between"
  },
  "pages_index_div65": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.primary
  },
  "pages_index_svg66": {
    "fill": "none",
    "stroke": marketing.colors.primary,
    "strokeWidth": "1.4"
  },
  "pages_index_rect67": {},
  "pages_index_circle68": {
    "fill": marketing.colors.primary
  },
  "pages_index_h369": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.onSurface
  },
  "pages_index_article70": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingRight": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingBottom": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "paddingLeft": {
      "default": "2rem",
      "@media (min-width: 768px)": "2.5rem"
    }
  },
  "pages_index_div71": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.cyberBlue
  },
  "pages_index_svg72": {
    "fill": "none",
    "stroke": marketing.colors.cyberBlue,
    "strokeWidth": "1.4"
  },
  "pages_index_code73": {
    "backgroundColor": marketing.colors.deepNavy,
    "paddingLeft": "0.375rem",
    "paddingRight": "0.375rem",
    "paddingTop": "0.125rem",
    "paddingBottom": "0.125rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "13px",
    "color": marketing.colors.primary
  },
  "pages_index_div74": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.primary
  },
  "pages_index_svg75": {
    "fill": "none",
    "stroke": marketing.colors.primaryFixed,
    "strokeWidth": "1.4"
  },
  "pages_index_section76": {
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"]
  },
  "pages_index_div77": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    }
  },
  "pages_index_div78": {
    "display": "grid",
    "alignItems": "flex-start",
    "gap": {
      "default": "3rem",
      "@media (min-width: 1024px)": "5rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 1024px)": "1.1fr 1fr"
    }
  },
  "pages_index_p79": {
    "marginBottom": "1.25rem",
    "fontSize": "1.125rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_p80": {
    "marginBottom": "1.5rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div81": {
    "marginBottom": "1.5rem",
    "overflowX": "auto",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.colors.deepNavy,
    "paddingLeft": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "13px",
    "color": marketing.colors.parchmentText
  },
  "pages_index_div82": {
    "marginTop": "0.5rem",
    "color": marketing.colors.outline
  },
  "pages_index_a83": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.cyberBlue["0.6"],
    "borderRightColor": marketing.alpha.cyberBlue["0.6"],
    "borderBottomColor": marketing.alpha.cyberBlue["0.6"],
    "borderLeftColor": marketing.alpha.cyberBlue["0.6"],
    "paddingLeft": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.cyberBlue,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "backgroundColor": {
      "default": null,
      ":hover": marketing.alpha.cyberBlue["0.05"]
    }
  },
  "pages_index_div84": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem"
  },
  "pages_index_div85": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.outline
  },
  "pages_index_ul86": {
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_li87": {
    "display": "flex",
    "gap": "0.75rem",
    "marginTop": {
      "default": null,
      ":not(:first-child)": "0.75rem"
    }
  },
  "pages_index_span88": {
    "flexShrink": "0",
    "fontFamily": marketing.typography.utilityMono,
    "color": marketing.colors.primary
  },
  "pages_index_section89": {
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.alpha.deepNavy["0.3"]
  },
  "pages_index_div90": {
    "marginBottom": "3.5rem",
    "display": "flex",
    "flexDirection": {
      "default": "column",
      "@media (min-width: 768px)": "row"
    },
    "justifyContent": "space-between",
    "gap": "1.5rem",
    "alignItems": {
      "default": null,
      "@media (min-width: 768px)": "flex-end"
    }
  },
  "pages_index_div91": {
    "maxWidth": "36rem"
  },
  "pages_index_h292": {
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.onSurface
  },
  "pages_index_a93": {
    "alignSelf": {
      "default": "flex-start",
      "@media (min-width: 768px)": "flex-end"
    },
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": {
      "default": marketing.colors.primary,
      ":hover": marketing.colors.cyberBlue
    }
  },
  "pages_index_div94": {
    "display": "grid",
    "gap": marketing.layout.gutter,
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))",
      "@media (min-width: 1024px)": "repeat(4, minmax(0, 1fr))"
    }
  },
  "pages_index_article95": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingBottom": "1.5rem",
    "paddingLeft": "1.5rem",
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "pages_index_div96": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.primary
  },
  "pages_index_p97": {
    "fontSize": "15px",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div98": {
    "marginBottom": "3.5rem",
    "maxWidth": "42rem"
  },
  "pages_index_div99": {
    "display": "grid",
    "gap": marketing.layout.gutter,
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))",
      "@media (min-width: 1024px)": "repeat(4, minmax(0, 1fr))"
    }
  },
  "pages_index_article100": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.1"],
    "borderRightColor": marketing.alpha.outlineVariant["0.1"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.1"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.1"],
    "backgroundColor": marketing.colors.surfaceContainerLow,
    "paddingTop": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingBottom": "1.5rem",
    "paddingLeft": "1.5rem"
  },
  "pages_index_h3101": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.25rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.primary
  },
  "pages_index_section102": {
    "position": "relative",
    "overflow": "hidden",
    "borderTopWidth": "1px",
    "borderBottomWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"]
  },
  "pages_index_div103": {
    "position": "absolute",
    "left": "50%",
    "top": "50%",
    "aspectRatio": "1 / 1",
    "width": "700px",
    "transform": "translate(-50%, -50%) rotate(0) skewX(0) skewY(0) scaleX(1) scaleY(1)",
    "borderRadius": "9999px",
    "backgroundColor": marketing.alpha.primary["0.05"],
    "filter": "blur(120px)"
  },
  "pages_index_div104": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "textAlign": "center"
  },
  "pages_index_h2105": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.primary
  },
  "pages_index_p106": {
    "marginBottom": "3rem",
    "fontSize": {
      "default": "1.125rem",
      "@media (min-width: 768px)": "1.25rem"
    },
    "lineHeight": {
      "default": "1.625",
      "@media (min-width: 768px)": "1.75rem"
    },
    "color": marketing.colors.parchmentText
  },
  "pages_index_div107": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "display": "grid",
    "maxWidth": "28rem",
    "gridTemplateColumns": "repeat(3, minmax(0, 1fr))",
    "gap": "1.5rem"
  },
  "pages_index_div108": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": {
      "default": "1.875rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "lineHeight": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "fontWeight": "700",
    "color": marketing.colors.primary
  },
  "pages_index_div109": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_div110": {
    "display": "grid",
    "alignItems": "flex-start",
    "gap": {
      "default": "3rem",
      "@media (min-width: 768px)": "5rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(2, minmax(0, 1fr))"
    }
  },
  "pages_index_h2111": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "3rem"
    },
    "lineHeight": {
      "default": "1.25",
      "@media (min-width: 768px)": "1"
    },
    "color": marketing.colors.onSurface
  },
  "pages_index_a112": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "700",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.inkBlack,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "filter": {
      "default": null,
      ":hover": "brightness(1.1)"
    }
  },
  "pages_index_div113": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainerLow,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem"
  },
  "pages_index_ol114": {
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_li115": {
    "display": "flex",
    "gap": "1rem",
    "marginTop": {
      "default": null,
      ":not(:first-child)": "1.25rem"
    }
  },
  "pages_index_span116": {
    "width": "1.5rem",
    "flexShrink": "0",
    "fontFamily": marketing.typography.utilityMono,
    "color": marketing.colors.primary
  },
  "pages_index_div117": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    }
  },
  "pages_index_div118": {
    "marginBottom": "3.5rem"
  },
  "pages_index_div119": {},
  "pages_index_details120": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "backgroundColor": marketing.colors.surfaceContainerLow,
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "marginTop": {
      "default": null,
      ":not(:first-child)": "1rem"
    }
  },
  "pages_index_summary121": {
    "display": "flex",
    "cursor": "pointer",
    "listStyleType": "none",
    "alignItems": "flex-start",
    "gap": "1rem",
    "paddingTop": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingBottom": "1.5rem",
    "paddingLeft": "1.5rem"
  },
  "pages_index_span122": {
    "marginTop": "0.25rem",
    "fontFamily": marketing.typography.utilityMono,
    "color": marketing.colors.primary,
    "transitionProperty": "transform",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "transform": {
      "default": "rotate(0deg)",
      [stylex.when.ancestor(":open")]: "rotate(45deg)"
    }
  },
  "pages_index_span123": {
    "flex": "1 1 0%",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.25rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.onSurface
  },
  "pages_index_p124": {
    "paddingLeft": "3.5rem",
    "paddingRight": "1.5rem",
    "paddingBottom": "1.5rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_section125": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "paddingBottom": {
      "default": "5rem",
      "@media (min-width: 768px)": "7rem"
    },
    "textAlign": "center"
  },
  "pages_index_p126": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "marginBottom": "2.5rem",
    "maxWidth": "36rem",
    "fontSize": "1.125rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_index_a127": {
    "display": "inline-flex",
    "alignItems": "center",
    "justifyContent": "center",
    "gap": "0.5rem",
    "backgroundColor": marketing.colors.brassGold,
    "paddingLeft": "2rem",
    "paddingRight": "2rem",
    "paddingTop": "1rem",
    "paddingBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "fontWeight": "700",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.inkBlack,
    "transitionProperty": "all",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    },
    "boxShadow": {
      "default": null,
      ":hover": `0 0 24px ${marketing.alpha.brassGold["0.35"]}`
    }
  },
  "pages_kontakt_section1": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    }
  },
  "pages_kontakt_div2": {
    "display": "grid",
    "gap": "1.5rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))"
    }
  },
  "pages_kontakt_a3": {
    "display": "block",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.primary["0.4"]
    },
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem",
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "pages_kontakt_h24": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.onSurface
  },
  "pages_kontakt_p5": {
    "marginBottom": "1rem",
    "fontSize": "15px",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_kontakt_span6": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": marketing.colors.primary
  },
  "pages_kontakt_div7": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.outline
  },
  "pages_kontakt_div8": {
    "display": "block",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainerLow,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem"
  },
  "pages_kontakt_a9": {
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "12px",
    "textTransform": "uppercase",
    "letterSpacing": "0.15em",
    "color": {
      "default": marketing.colors.primary,
      ":hover": marketing.colors.cyberBlue
    }
  },
  "pages_kontakt_a10": {
    "display": "block",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.error["0.6"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.error["0.6"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.error["0.6"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.15"],
      ":hover": marketing.alpha.error["0.6"]
    },
    "backgroundColor": marketing.colors.surfaceContainerLow,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem",
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "pages_kontakt_div11": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.error
  },
  "pages_kontakt_section12": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px",
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "4rem"
    }
  },
  "pages_docs_foersteFaktura_em1": {},
  "pages_docs_index_li1": {
    "marginBottom": "0.5rem",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_docs_index_br2": {},
  "pages_docs_index_span3": {
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_docs_index_span4": {},
  "pages_vaerktoej_momsberegner_div1": {
    "display": "grid",
    "alignItems": "flex-start",
    "gap": {
      "default": "2rem",
      "@media (min-width: 1024px)": "3rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 1024px)": "1fr 0.9fr"
    }
  },
  "pages_vaerktoej_momsberegner_div2": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": {
      "default": "1.75rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "paddingRight": {
      "default": "1.75rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "paddingBottom": {
      "default": "1.75rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "paddingLeft": {
      "default": "1.75rem",
      "@media (min-width: 768px)": "2.25rem"
    }
  },
  "pages_vaerktoej_momsberegner_form3": {},
  "pages_vaerktoej_momsberegner_div4": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "1.5rem"
    }
  },
  "pages_vaerktoej_momsberegner_label5": {
    "marginBottom": "0.5rem",
    "display": "block",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.primary
  },
  "pages_vaerktoej_momsberegner_input6": {
    "width": "100%",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.4"],
    "borderRightColor": marketing.alpha.outlineVariant["0.4"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.4"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.4"],
    "backgroundColor": marketing.colors.inkBlack,
    "paddingLeft": "1rem",
    "paddingRight": "1rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "1.125rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.onSurface
  },
  "pages_vaerktoej_momsberegner_fieldset7": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "1.5rem"
    }
  },
  "pages_vaerktoej_momsberegner_legend8": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.primary
  },
  "pages_vaerktoej_momsberegner_label9": {
    "display": "flex",
    "alignItems": "center",
    "gap": "0.75rem",
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.3"],
    "borderRightColor": marketing.alpha.outlineVariant["0.3"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.3"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.3"],
    "backgroundColor": marketing.colors.deepNavy,
    "paddingLeft": "1rem",
    "paddingRight": "1rem",
    "paddingTop": "0.75rem",
    "paddingBottom": "0.75rem"
  },
  "pages_vaerktoej_momsberegner_input10": {},
  "pages_vaerktoej_momsberegner_div11": {
    "marginTop": "2rem",
    "display": "grid",
    "gap": "1rem",
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(3, minmax(0, 1fr))"
    }
  },
  "pages_vaerktoej_momsberegner_div12": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.2"],
    "borderRightColor": marketing.alpha.outlineVariant["0.2"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.2"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.2"],
    "backgroundColor": marketing.colors.inkBlack,
    "paddingTop": "1.25rem",
    "paddingRight": "1.25rem",
    "paddingBottom": "1.25rem",
    "paddingLeft": "1.25rem"
  },
  "pages_vaerktoej_momsberegner_div13": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "10px",
    "textTransform": "uppercase",
    "letterSpacing": "0.16em",
    "color": marketing.colors.outline
  },
  "pages_vaerktoej_momsberegner_div14": {
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.onSurface
  },
  "pages_vaerktoej_momsberegner_div15": {
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.primary
  },
  "pages_vaerktoej_momsberegner_article16": {
    "color": marketing.colors.onSurfaceVariant,
    "lineHeight": "1.7",
    "fontSize": "17px"
  },
  "pages_funktioner_div1": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "3rem",
      "@media (min-width: 768px)": "5rem"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "5rem"
    }
  },
  "pages_funktioner_section2": {
    "marginTop": {
      "default": null,
      ":not(:first-child)": "4rem"
    }
  },
  "pages_funktioner_div3": {
    "marginBottom": "2rem",
    "maxWidth": "42rem"
  },
  "pages_funktioner_h24": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "1.875rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "lineHeight": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "color": marketing.colors.primary
  },
  "pages_funktioner_p5": {
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_funktioner_div6": {
    "display": "grid",
    "gap": marketing.layout.gutter,
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 640px)": "repeat(2, minmax(0, 1fr))",
      "@media (min-width: 1024px)": "repeat(3, minmax(0, 1fr))"
    }
  },
  "pages_funktioner_article7": {
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderRightColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderBottomColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "borderLeftColor": {
      "default": marketing.alpha.outlineVariant["0.1"],
      ":hover": marketing.alpha.primary["0.3"]
    },
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "1.5rem",
    "paddingRight": "1.5rem",
    "paddingBottom": "1.5rem",
    "paddingLeft": "1.5rem",
    "transitionProperty": "color, background-color, border-color, text-decoration-color, fill, stroke",
    "transitionTimingFunction": "cubic-bezier(0.4, 0, 0.2, 1)",
    "transitionDuration": {
      "default": "150ms",
      "@media (prefers-reduced-motion: reduce)": "0s"
    }
  },
  "pages_funktioner_h38": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.25rem",
    "lineHeight": "1.75rem",
    "color": marketing.colors.onSurface
  },
  "pages_saadanVirkerDet_section1": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": marketing.layout.maxWidth,
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "3rem",
      "@media (min-width: 768px)": "5rem"
    },
    "paddingBottom": {
      "default": "3rem",
      "@media (min-width: 768px)": "5rem"
    }
  },
  "pages_saadanVirkerDet_div2": {
    "display": "grid",
    "gap": {
      "default": "1.5rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "gridTemplateColumns": {
      "default": null,
      "@media (min-width: 768px)": "repeat(2, minmax(0, 1fr))"
    }
  },
  "pages_saadanVirkerDet_article3": {
    "boxShadow": `inset 0 0 15px ${marketing.alpha.cyberBlue["0.05"]}`,
    "borderTopWidth": "1px",
    "borderRightWidth": "1px",
    "borderBottomWidth": "1px",
    "borderLeftWidth": "1px",
    "borderTopStyle": "solid",
    "borderRightStyle": "solid",
    "borderBottomStyle": "solid",
    "borderLeftStyle": "solid",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "backgroundColor": marketing.colors.surfaceContainer,
    "paddingTop": "2rem",
    "paddingRight": "2rem",
    "paddingBottom": "2rem",
    "paddingLeft": "2rem"
  },
  "pages_saadanVirkerDet_div4": {
    "marginBottom": "1rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.primary
  },
  "pages_saadanVirkerDet_h25": {
    "marginBottom": "0.75rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": "1.5rem",
    "lineHeight": "2rem",
    "color": marketing.colors.onSurface
  },
  "pages_saadanVirkerDet_div6": {
    "marginLeft": "auto",
    "marginRight": "auto",
    "maxWidth": "48rem",
    "paddingLeft": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingRight": {
      "default": "16px",
      "@media (min-width: 768px)": "64px"
    },
    "paddingTop": {
      "default": "4rem",
      "@media (min-width: 768px)": "6rem"
    },
    "paddingBottom": {
      "default": "4rem",
      "@media (min-width: 768px)": "6rem"
    }
  },
  "pages_saadanVirkerDet_h27": {
    "marginBottom": "1.5rem",
    "fontFamily": marketing.typography.utilityHeadline,
    "fontSize": {
      "default": "1.875rem",
      "@media (min-width: 768px)": "2.25rem"
    },
    "lineHeight": {
      "default": "2.25rem",
      "@media (min-width: 768px)": "2.5rem"
    },
    "color": marketing.colors.primary
  },
  "pages_saadanVirkerDet_dl8": {},
  "pages_saadanVirkerDet_dt9": {
    "marginBottom": "0.5rem",
    "fontFamily": marketing.typography.utilityMono,
    "fontSize": "11px",
    "textTransform": "uppercase",
    "letterSpacing": "0.2em",
    "color": marketing.colors.cyberBlue
  },
  "pages_saadanVirkerDet_dd10": {
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  },
  "pages_saadanVirkerDet_div11": {
    "marginTop": "3rem",
    "borderTopWidth": "1px",
    "borderTopColor": marketing.alpha.outlineVariant["0.15"],
    "borderRightColor": marketing.alpha.outlineVariant["0.15"],
    "borderBottomColor": marketing.alpha.outlineVariant["0.15"],
    "borderLeftColor": marketing.alpha.outlineVariant["0.15"],
    "paddingTop": "2rem"
  },
  "pages_saadanVirkerDet_p12": {
    "marginBottom": "1rem",
    "lineHeight": "1.625",
    "color": marketing.colors.onSurfaceVariant
  }
});

export const attributes = {
  components_Footer_footer1: stylex.attrs(base.element, styles.components_Footer_footer1),
  components_Footer_div2: stylex.attrs(base.element, styles.components_Footer_div2),
  components_Footer_div3: stylex.attrs(base.element, styles.components_Footer_div3),
  components_Footer_div4: stylex.attrs(base.element, styles.components_Footer_div4),
  components_Footer_p5: stylex.attrs(base.element, base.p, styles.components_Footer_p5),
  components_Footer_a6: stylex.attrs(base.element, base.a, styles.components_Footer_a6),
  components_Footer_svg7: stylex.attrs(base.element, base.svg, styles.components_Footer_svg7),
  components_Footer_path8: stylex.attrs(base.element, styles.components_Footer_path8),
  components_Footer_div9: stylex.attrs(base.element, styles.components_Footer_div9),
  components_Footer_div10: stylex.attrs(base.element, styles.components_Footer_div10),
  components_Footer_ul11: stylex.attrs(base.element, base.ul, styles.components_Footer_ul11),
  components_Footer_li12: stylex.attrs(base.element, styles.components_Footer_li12),
  components_Footer_a13: stylex.attrs(base.element, base.a, styles.components_Footer_a13),
  components_Footer_div14: stylex.attrs(base.element, styles.components_Footer_div14),
  components_Footer_div15: stylex.attrs(base.element, styles.components_Footer_div15),
  components_Footer_div16: stylex.attrs(base.element, styles.components_Footer_div16),
  components_Footer_a17: stylex.attrs(base.element, base.a, styles.components_Footer_a17),
  components_PageHero_section1: stylex.attrs(base.element, styles.components_PageHero_section1),
  components_PageHero_div2: stylex.attrs(base.element, styles.components_PageHero_div2),
  components_PageHero_div3: stylex.attrs(base.element, styles.components_PageHero_div3),
  components_PageHero_div4: stylex.attrs(base.element, styles.components_PageHero_div4),
  components_PageHero_div5: stylex.attrs(base.element, styles.components_PageHero_div5),
  components_PageHero_div6: stylex.attrs(base.element, styles.components_PageHero_div6),
  components_PageHero_h17: stylex.attrs(base.element, base.h1, styles.components_PageHero_h17),
  components_PageHero_p8: stylex.attrs(base.element, base.p, styles.components_PageHero_p8),
  components_KnowledgeSectionLinks_aside1: stylex.attrs(base.element, styles.components_KnowledgeSectionLinks_aside1),
  components_KnowledgeSectionLinks_div2: stylex.attrs(base.element, styles.components_KnowledgeSectionLinks_div2),
  components_KnowledgeSectionLinks_div3: stylex.attrs(base.element, styles.components_KnowledgeSectionLinks_div3),
  components_KnowledgeSectionLinks_a4: stylex.attrs(base.element, base.a, styles.components_KnowledgeSectionLinks_a4),
  components_KnowledgeSectionLinks_a5: stylex.attrs(base.element, base.a, styles.components_KnowledgeSectionLinks_a5),
  components_Header_header1: stylex.attrs(base.element, styles.components_Header_header1),
  components_Header_div2: stylex.attrs(base.element, styles.components_Header_div2),
  components_Header_a3: stylex.attrs(base.element, base.a, styles.components_Header_a3),
  components_Header_nav4: stylex.attrs(base.element, styles.components_Header_nav4),
  components_Header_a5: stylex.attrs(base.element, base.a, styles.components_Header_a5),
  components_Header_a6: stylex.attrs(base.element, base.a, styles.components_Header_a6),
  components_Header_div7: stylex.attrs(base.element, styles.components_Header_div7),
  components_Header_a8: stylex.attrs(base.element, base.a, styles.components_Header_a8),
  components_Header_button9: stylex.attrs(base.element, base.button, styles.components_Header_button9),
  components_Header_svg10: stylex.attrs(base.element, base.svg, styles.components_Header_svg10),
  components_Header_line11: stylex.attrs(base.element, styles.components_Header_line11),
  components_Header_nav12: stylex.attrs(base.element, styles.components_Header_nav12),
  components_Header_ul13: stylex.attrs(base.element, base.ul, styles.components_Header_ul13),
  components_Header_li14: stylex.attrs(base.element, styles.components_Header_li14),
  components_Header_a15: stylex.attrs(base.element, base.a, styles.components_Header_a15),
  components_Header_li16: stylex.attrs(base.element, styles.components_Header_li16),
  components_Header_a17: stylex.attrs(base.element, base.a, styles.components_Header_a17),
  components_SeoEvidence_aside1: stylex.attrs(base.element, styles.components_SeoEvidence_aside1),
  components_SeoEvidence_p2: stylex.attrs(base.element, base.p, styles.components_SeoEvidence_p2),
  components_SeoEvidence_a3: stylex.attrs(base.element, base.a, styles.components_SeoEvidence_a3),
  components_SeoEvidence_time4: stylex.attrs(base.element, styles.components_SeoEvidence_time4),
  components_SeoEvidence_div5: stylex.attrs(base.element, styles.components_SeoEvidence_div5),
  components_SeoEvidence_div6: stylex.attrs(base.element, styles.components_SeoEvidence_div6),
  components_SeoEvidence_div7: stylex.attrs(base.element, styles.components_SeoEvidence_div7),
  components_SeoEvidence_dl8: stylex.attrs(base.element, base.dl, styles.components_SeoEvidence_dl8),
  components_SeoEvidence_div9: stylex.attrs(base.element, styles.components_SeoEvidence_div9),
  components_SeoEvidence_dt10: stylex.attrs(base.element, styles.components_SeoEvidence_dt10),
  components_SeoEvidence_dd11: stylex.attrs(base.element, base.dd, styles.components_SeoEvidence_dd11),
  components_SeoEvidence_span12: stylex.attrs(base.element, styles.components_SeoEvidence_span12),
  components_SeoEvidence_div13: stylex.attrs(base.element, styles.components_SeoEvidence_div13),
  components_SeoEvidence_div14: stylex.attrs(base.element, styles.components_SeoEvidence_div14),
  components_SeoEvidence_ul15: stylex.attrs(base.element, base.ul, styles.components_SeoEvidence_ul15),
  components_SeoEvidence_li16: stylex.attrs(base.element, styles.components_SeoEvidence_li16),
  components_SeoEvidence_a17: stylex.attrs(base.element, base.a, styles.components_SeoEvidence_a17),
  layouts_BaseLayout_html1: stylex.attrs(base.element, base.html, styles.layouts_BaseLayout_html1),
  layouts_BaseLayout_body2: stylex.attrs(base.element, base.body, styles.layouts_BaseLayout_body2),
  layouts_BaseLayout_a3: stylex.attrs(base.element, base.a, styles.layouts_BaseLayout_a3),
  layouts_BaseLayout_main4: stylex.attrs(base.element, styles.layouts_BaseLayout_main4),
  layouts_BaseLayout_nav5: stylex.attrs(base.element, styles.layouts_BaseLayout_nav5),
  layouts_BaseLayout_ol6: stylex.attrs(base.element, base.ol, styles.layouts_BaseLayout_ol6),
  layouts_BaseLayout_li7: stylex.attrs(base.element, styles.layouts_BaseLayout_li7),
  layouts_BaseLayout_span8: stylex.attrs(base.element, styles.layouts_BaseLayout_span8),
  layouts_BaseLayout_span9: stylex.attrs(base.element, styles.layouts_BaseLayout_span9),
  layouts_BaseLayout_div10: stylex.attrs(base.element, styles.layouts_BaseLayout_div10),
  layouts_BaseLayout_span11: stylex.attrs(base.element, styles.layouts_BaseLayout_span11),
  layouts_BaseLayout_span12: stylex.attrs(base.element, styles.layouts_BaseLayout_span12),
  layouts_BaseLayout_time13: stylex.attrs(base.element, styles.layouts_BaseLayout_time13),
  layouts_BaseLayout_time14: stylex.attrs(base.element, styles.layouts_BaseLayout_time14),
  pages_viden_regnskab_skatteopgoerelse_article1: stylex.attrs(base.element, styles.pages_viden_regnskab_skatteopgoerelse_article1),
  pages_viden_regnskab_skatteopgoerelse_p2: stylex.attrs(base.element, base.p, styles.pages_viden_regnskab_skatteopgoerelse_p2),
  pages_viden_regnskab_skatteopgoerelse_h23: stylex.attrs(base.element, base.h2, styles.pages_viden_regnskab_skatteopgoerelse_h23),
  pages_viden_saadanBogfoererDu_leverandoerfaktura_ul1: stylex.attrs(base.element, base.ul, styles.pages_viden_saadanBogfoererDu_leverandoerfaktura_ul1),
  pages_viden_saadanBogfoererDu_leverandoerfaktura_li2: stylex.attrs(base.element, styles.pages_viden_saadanBogfoererDu_leverandoerfaktura_li2),
  pages_viden_saadanBogfoererDu_udlaeg_a1: stylex.attrs(base.element, base.a, styles.pages_viden_saadanBogfoererDu_udlaeg_a1),
  pages_viden_saadanBogfoererDu_udlaeg_pre2: stylex.attrs(base.element, base.pre, styles.pages_viden_saadanBogfoererDu_udlaeg_pre2),
  pages_viden_saadanBogfoererDu_udlaeg_code3: stylex.attrs(base.element, base.code, styles.pages_viden_saadanBogfoererDu_udlaeg_code3),
  pages_viden_fakturering_eFakturaOffentlig_h31: stylex.attrs(base.element, base.h3, styles.pages_viden_fakturering_eFakturaOffentlig_h31),
  pages_viden_index_section1: stylex.attrs(base.element, styles.pages_viden_index_section1),
  pages_viden_index_div2: stylex.attrs(base.element, styles.pages_viden_index_div2),
  pages_viden_index_div3: stylex.attrs(base.element, styles.pages_viden_index_div3),
  pages_viden_index_article4: stylex.attrs(base.element, styles.pages_viden_index_article4),
  pages_viden_index_h25: stylex.attrs(base.element, base.h2, styles.pages_viden_index_h25),
  pages_viden_index_ul6: stylex.attrs(base.element, base.ul, styles.pages_viden_index_ul6),
  pages_viden_index_li7: stylex.attrs(base.element, styles.pages_viden_index_li7),
  pages_viden_index_a8: stylex.attrs(base.element, base.a, styles.pages_viden_index_a8),
  pages_viden_frister_moms_strong1: stylex.attrs(base.element, base.strong, styles.pages_viden_frister_moms_strong1),
  pages_bogfoeringsprogram_ol1: stylex.attrs(base.element, base.ol, styles.pages_bogfoeringsprogram_ol1),
  pages_privacyPolicy_article1: stylex.attrs(base.element, styles.pages_privacyPolicy_article1),
  pages_privacyPolicy_h12: stylex.attrs(base.element, base.h1, styles.pages_privacyPolicy_h12),
  pages_sikkerhed_code1: stylex.attrs(base.element, base.code, styles.pages_sikkerhed_code1),
  pages_404_section1: stylex.attrs(base.element, styles.pages_404_section1),
  pages_404_div2: stylex.attrs(base.element, styles.pages_404_div2),
  pages_404_h13: stylex.attrs(base.element, base.h1, styles.pages_404_h13),
  pages_404_p4: stylex.attrs(base.element, base.p, styles.pages_404_p4),
  pages_404_div5: stylex.attrs(base.element, styles.pages_404_div5),
  pages_404_a6: stylex.attrs(base.element, base.a, styles.pages_404_a6),
  pages_404_a7: stylex.attrs(base.element, base.a, styles.pages_404_a7),
  pages_index_section1: stylex.attrs(base.element, styles.pages_index_section1),
  pages_index_div2: stylex.attrs(base.element, styles.pages_index_div2),
  pages_index_div3: stylex.attrs(base.element, styles.pages_index_div3),
  pages_index_div4: stylex.attrs(base.element, styles.pages_index_div4),
  pages_index_div5: stylex.attrs(base.element, styles.pages_index_div5),
  pages_index_span6: stylex.attrs(base.element, styles.pages_index_span6),
  pages_index_span7: stylex.attrs(base.element, styles.pages_index_span7),
  pages_index_h18: stylex.attrs(base.element, base.h1, styles.pages_index_h18),
  pages_index_br9: stylex.attrs(base.element, styles.pages_index_br9),
  pages_index_p10: stylex.attrs(base.element, base.p, styles.pages_index_p10),
  pages_index_strong11: stylex.attrs(base.element, base.strong, styles.pages_index_strong11),
  pages_index_p12: stylex.attrs(base.element, base.p, styles.pages_index_p12),
  pages_index_strong13: stylex.attrs(base.element, base.strong, styles.pages_index_strong13),
  pages_index_p14: stylex.attrs(base.element, base.p, styles.pages_index_p14),
  pages_index_div15: stylex.attrs(base.element, styles.pages_index_div15),
  pages_index_a16: stylex.attrs(base.element, base.a, styles.pages_index_a16),
  pages_index_a17: stylex.attrs(base.element, base.a, styles.pages_index_a17),
  pages_index_div18: stylex.attrs(base.element, styles.pages_index_div18),
  pages_index_div19: stylex.attrs(base.element, styles.pages_index_div19),
  pages_index_div20: stylex.attrs(base.element, styles.pages_index_div20),
  pages_index_span21: stylex.attrs(base.element, styles.pages_index_span21),
  pages_index_span22: stylex.attrs(base.element, styles.pages_index_span22),
  pages_index_section23: stylex.attrs(base.element, styles.pages_index_section23),
  pages_index_section24: stylex.attrs(base.element, styles.pages_index_section24),
  pages_index_div25: stylex.attrs(base.element, styles.pages_index_div25),
  pages_index_div26: stylex.attrs(base.element, styles.pages_index_div26),
  pages_index_div27: stylex.attrs(base.element, styles.pages_index_div27),
  pages_index_div28: stylex.attrs(base.element, styles.pages_index_div28),
  pages_index_h229: stylex.attrs(base.element, base.h2, styles.pages_index_h229),
  pages_index_div30: stylex.attrs(base.element, styles.pages_index_div30),
  pages_index_section31: stylex.attrs(base.element, styles.pages_index_section31),
  pages_index_div32: stylex.attrs(base.element, styles.pages_index_div32),
  pages_index_div33: stylex.attrs(base.element, styles.pages_index_div33),
  pages_index_h234: stylex.attrs(base.element, base.h2, styles.pages_index_h234),
  pages_index_p35: stylex.attrs(base.element, base.p, styles.pages_index_p35),
  pages_index_p36: stylex.attrs(base.element, base.p, styles.pages_index_p36),
  pages_index_div37: stylex.attrs(base.element, styles.pages_index_div37),
  pages_index_article38: stylex.attrs(base.element, styles.pages_index_article38),
  pages_index_div39_live: stylex.attrs(base.element, styles.pages_index_div39_live),
  pages_index_div40_now: stylex.attrs(base.element, styles.pages_index_div40_now),
  pages_index_div41_next: stylex.attrs(base.element, styles.pages_index_div41_next),
  pages_index_div42_future: stylex.attrs(base.element, styles.pages_index_div42_future),
  pages_index_div43: stylex.attrs(base.element, styles.pages_index_div43),
  pages_index_div44: stylex.attrs(base.element, styles.pages_index_div44),
  pages_index_span45: stylex.attrs(base.element, styles.pages_index_span45),
  pages_index_span46: stylex.attrs(base.element, styles.pages_index_span46),
  pages_index_h347: stylex.attrs(base.element, base.h3, styles.pages_index_h347),
  pages_index_p48: stylex.attrs(base.element, base.p, styles.pages_index_p48),
  pages_index_div49: stylex.attrs(base.element, styles.pages_index_div49),
  pages_index_div50: stylex.attrs(base.element, styles.pages_index_div50),
  pages_index_h251: stylex.attrs(base.element, base.h2, styles.pages_index_h251),
  pages_index_p52: stylex.attrs(base.element, base.p, styles.pages_index_p52),
  pages_index_div53: stylex.attrs(base.element, styles.pages_index_div53),
  pages_index_pre54: stylex.attrs(base.element, base.pre, styles.pages_index_pre54),
  pages_index_code55: stylex.attrs(base.element, base.code, styles.pages_index_code55),
  pages_index_div56: stylex.attrs(base.element, styles.pages_index_div56),
  pages_index_a57: stylex.attrs(base.element, base.a, styles.pages_index_a57),
  pages_index_a58: stylex.attrs(base.element, base.a, styles.pages_index_a58),
  pages_index_div59: stylex.attrs(base.element, styles.pages_index_div59),
  pages_index_h260: stylex.attrs(base.element, base.h2, styles.pages_index_h260),
  pages_index_p61: stylex.attrs(base.element, base.p, styles.pages_index_p61),
  pages_index_div62: stylex.attrs(base.element, styles.pages_index_div62),
  pages_index_article63: stylex.attrs(base.element, styles.pages_index_article63),
  pages_index_div64: stylex.attrs(base.element, styles.pages_index_div64),
  pages_index_div65: stylex.attrs(base.element, styles.pages_index_div65),
  pages_index_svg66: stylex.attrs(base.element, base.svg, styles.pages_index_svg66),
  pages_index_rect67: stylex.attrs(base.element, styles.pages_index_rect67),
  pages_index_circle68: stylex.attrs(base.element, styles.pages_index_circle68),
  pages_index_h369: stylex.attrs(base.element, base.h3, styles.pages_index_h369),
  pages_index_article70: stylex.attrs(base.element, styles.pages_index_article70),
  pages_index_div71: stylex.attrs(base.element, styles.pages_index_div71),
  pages_index_svg72: stylex.attrs(base.element, base.svg, styles.pages_index_svg72),
  pages_index_code73: stylex.attrs(base.element, base.code, styles.pages_index_code73),
  pages_index_div74: stylex.attrs(base.element, styles.pages_index_div74),
  pages_index_svg75: stylex.attrs(base.element, base.svg, styles.pages_index_svg75),
  pages_index_section76: stylex.attrs(base.element, styles.pages_index_section76),
  pages_index_div77: stylex.attrs(base.element, styles.pages_index_div77),
  pages_index_div78: stylex.attrs(base.element, styles.pages_index_div78),
  pages_index_p79: stylex.attrs(base.element, base.p, styles.pages_index_p79),
  pages_index_p80: stylex.attrs(base.element, base.p, styles.pages_index_p80),
  pages_index_div81: stylex.attrs(base.element, styles.pages_index_div81),
  pages_index_div82: stylex.attrs(base.element, styles.pages_index_div82),
  pages_index_a83: stylex.attrs(base.element, base.a, styles.pages_index_a83),
  pages_index_div84: stylex.attrs(base.element, styles.pages_index_div84),
  pages_index_div85: stylex.attrs(base.element, styles.pages_index_div85),
  pages_index_ul86: stylex.attrs(base.element, base.ul, styles.pages_index_ul86),
  pages_index_li87: stylex.attrs(base.element, styles.pages_index_li87),
  pages_index_span88: stylex.attrs(base.element, styles.pages_index_span88),
  pages_index_section89: stylex.attrs(base.element, styles.pages_index_section89),
  pages_index_div90: stylex.attrs(base.element, styles.pages_index_div90),
  pages_index_div91: stylex.attrs(base.element, styles.pages_index_div91),
  pages_index_h292: stylex.attrs(base.element, base.h2, styles.pages_index_h292),
  pages_index_a93: stylex.attrs(base.element, base.a, styles.pages_index_a93),
  pages_index_div94: stylex.attrs(base.element, styles.pages_index_div94),
  pages_index_article95: stylex.attrs(base.element, styles.pages_index_article95),
  pages_index_div96: stylex.attrs(base.element, styles.pages_index_div96),
  pages_index_p97: stylex.attrs(base.element, base.p, styles.pages_index_p97),
  pages_index_div98: stylex.attrs(base.element, styles.pages_index_div98),
  pages_index_div99: stylex.attrs(base.element, styles.pages_index_div99),
  pages_index_article100: stylex.attrs(base.element, styles.pages_index_article100),
  pages_index_h3101: stylex.attrs(base.element, base.h3, styles.pages_index_h3101),
  pages_index_section102: stylex.attrs(base.element, styles.pages_index_section102),
  pages_index_div103: stylex.attrs(base.element, styles.pages_index_div103),
  pages_index_div104: stylex.attrs(base.element, styles.pages_index_div104),
  pages_index_h2105: stylex.attrs(base.element, base.h2, styles.pages_index_h2105),
  pages_index_p106: stylex.attrs(base.element, base.p, styles.pages_index_p106),
  pages_index_div107: stylex.attrs(base.element, styles.pages_index_div107),
  pages_index_div108: stylex.attrs(base.element, styles.pages_index_div108),
  pages_index_div109: stylex.attrs(base.element, styles.pages_index_div109),
  pages_index_div110: stylex.attrs(base.element, styles.pages_index_div110),
  pages_index_h2111: stylex.attrs(base.element, base.h2, styles.pages_index_h2111),
  pages_index_a112: stylex.attrs(base.element, base.a, styles.pages_index_a112),
  pages_index_div113: stylex.attrs(base.element, styles.pages_index_div113),
  pages_index_ol114: stylex.attrs(base.element, base.ol, styles.pages_index_ol114),
  pages_index_li115: stylex.attrs(base.element, styles.pages_index_li115),
  pages_index_span116: stylex.attrs(base.element, styles.pages_index_span116),
  pages_index_div117: stylex.attrs(base.element, styles.pages_index_div117),
  pages_index_div118: stylex.attrs(base.element, styles.pages_index_div118),
  pages_index_div119: stylex.attrs(base.element, styles.pages_index_div119),
  pages_index_details120: stylex.attrs(base.element, styles.pages_index_details120, stylex.defaultMarker()),
  pages_index_summary121: stylex.attrs(base.element, base.summary, styles.pages_index_summary121),
  pages_index_span122: stylex.attrs(base.element, styles.pages_index_span122),
  pages_index_span123: stylex.attrs(base.element, styles.pages_index_span123),
  pages_index_p124: stylex.attrs(base.element, base.p, styles.pages_index_p124),
  pages_index_section125: stylex.attrs(base.element, styles.pages_index_section125),
  pages_index_p126: stylex.attrs(base.element, base.p, styles.pages_index_p126),
  pages_index_a127: stylex.attrs(base.element, base.a, styles.pages_index_a127),
  pages_kontakt_section1: stylex.attrs(base.element, styles.pages_kontakt_section1),
  pages_kontakt_div2: stylex.attrs(base.element, styles.pages_kontakt_div2),
  pages_kontakt_a3: stylex.attrs(base.element, base.a, styles.pages_kontakt_a3),
  pages_kontakt_h24: stylex.attrs(base.element, base.h2, styles.pages_kontakt_h24),
  pages_kontakt_p5: stylex.attrs(base.element, base.p, styles.pages_kontakt_p5),
  pages_kontakt_span6: stylex.attrs(base.element, styles.pages_kontakt_span6),
  pages_kontakt_div7: stylex.attrs(base.element, styles.pages_kontakt_div7),
  pages_kontakt_div8: stylex.attrs(base.element, styles.pages_kontakt_div8),
  pages_kontakt_a9: stylex.attrs(base.element, base.a, styles.pages_kontakt_a9),
  pages_kontakt_a10: stylex.attrs(base.element, base.a, styles.pages_kontakt_a10),
  pages_kontakt_div11: stylex.attrs(base.element, styles.pages_kontakt_div11),
  pages_kontakt_section12: stylex.attrs(base.element, styles.pages_kontakt_section12),
  pages_docs_foersteFaktura_em1: stylex.attrs(base.element, styles.pages_docs_foersteFaktura_em1),
  pages_docs_index_li1: stylex.attrs(base.element, styles.pages_docs_index_li1),
  pages_docs_index_br2: stylex.attrs(base.element, styles.pages_docs_index_br2),
  pages_docs_index_span3: stylex.attrs(base.element, styles.pages_docs_index_span3),
  pages_docs_index_span4: stylex.attrs(base.element, styles.pages_docs_index_span4),
  pages_vaerktoej_momsberegner_div1: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div1),
  pages_vaerktoej_momsberegner_div2: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div2),
  pages_vaerktoej_momsberegner_form3: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_form3),
  pages_vaerktoej_momsberegner_div4: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div4),
  pages_vaerktoej_momsberegner_label5: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_label5),
  pages_vaerktoej_momsberegner_input6: stylex.attrs(base.element, base.input, styles.pages_vaerktoej_momsberegner_input6),
  pages_vaerktoej_momsberegner_fieldset7: stylex.attrs(base.element, base.fieldset, styles.pages_vaerktoej_momsberegner_fieldset7),
  pages_vaerktoej_momsberegner_legend8: stylex.attrs(base.element, base.legend, styles.pages_vaerktoej_momsberegner_legend8),
  pages_vaerktoej_momsberegner_label9: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_label9),
  pages_vaerktoej_momsberegner_input10: stylex.attrs(base.element, base.input, styles.pages_vaerktoej_momsberegner_input10),
  pages_vaerktoej_momsberegner_div11: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div11),
  pages_vaerktoej_momsberegner_div12: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div12),
  pages_vaerktoej_momsberegner_div13: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div13),
  pages_vaerktoej_momsberegner_div14: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div14),
  pages_vaerktoej_momsberegner_div15: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_div15),
  pages_vaerktoej_momsberegner_article16: stylex.attrs(base.element, styles.pages_vaerktoej_momsberegner_article16),
  pages_funktioner_div1: stylex.attrs(base.element, styles.pages_funktioner_div1),
  pages_funktioner_section2: stylex.attrs(base.element, styles.pages_funktioner_section2),
  pages_funktioner_div3: stylex.attrs(base.element, styles.pages_funktioner_div3),
  pages_funktioner_h24: stylex.attrs(base.element, base.h2, styles.pages_funktioner_h24),
  pages_funktioner_p5: stylex.attrs(base.element, base.p, styles.pages_funktioner_p5),
  pages_funktioner_div6: stylex.attrs(base.element, styles.pages_funktioner_div6),
  pages_funktioner_article7: stylex.attrs(base.element, styles.pages_funktioner_article7),
  pages_funktioner_h38: stylex.attrs(base.element, base.h3, styles.pages_funktioner_h38),
  pages_saadanVirkerDet_section1: stylex.attrs(base.element, styles.pages_saadanVirkerDet_section1),
  pages_saadanVirkerDet_div2: stylex.attrs(base.element, styles.pages_saadanVirkerDet_div2),
  pages_saadanVirkerDet_article3: stylex.attrs(base.element, styles.pages_saadanVirkerDet_article3),
  pages_saadanVirkerDet_div4: stylex.attrs(base.element, styles.pages_saadanVirkerDet_div4),
  pages_saadanVirkerDet_h25: stylex.attrs(base.element, base.h2, styles.pages_saadanVirkerDet_h25),
  pages_saadanVirkerDet_div6: stylex.attrs(base.element, styles.pages_saadanVirkerDet_div6),
  pages_saadanVirkerDet_h27: stylex.attrs(base.element, base.h2, styles.pages_saadanVirkerDet_h27),
  pages_saadanVirkerDet_dl8: stylex.attrs(base.element, base.dl, styles.pages_saadanVirkerDet_dl8),
  pages_saadanVirkerDet_dt9: stylex.attrs(base.element, styles.pages_saadanVirkerDet_dt9),
  pages_saadanVirkerDet_dd10: stylex.attrs(base.element, base.dd, styles.pages_saadanVirkerDet_dd10),
  pages_saadanVirkerDet_div11: stylex.attrs(base.element, styles.pages_saadanVirkerDet_div11),
  pages_saadanVirkerDet_p12: stylex.attrs(base.element, base.p, styles.pages_saadanVirkerDet_p12),
};
