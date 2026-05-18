---
name: Data-driven Trust
colors:
  surface: '#10131a'
  surface-dim: '#10131a'
  surface-bright: '#363940'
  surface-container-lowest: '#0b0e14'
  surface-container-low: '#191c22'
  surface-container: '#1d2026'
  surface-container-high: '#272a31'
  surface-container-highest: '#32353c'
  on-surface: '#e1e2eb'
  on-surface-variant: '#d1c5b4'
  inverse-surface: '#e1e2eb'
  inverse-on-surface: '#2e3037'
  outline: '#9a8f80'
  outline-variant: '#4e4639'
  surface-tint: '#e9c176'
  primary: '#e9c176'
  on-primary: '#412d00'
  primary-container: '#c5a059'
  on-primary-container: '#4e3700'
  inverse-primary: '#775a19'
  secondary: '#a6e6ff'
  on-secondary: '#003543'
  secondary-container: '#14d1ff'
  on-secondary-container: '#00566b'
  tertiary: '#bec6e0'
  on-tertiary: '#283044'
  tertiary-container: '#9da5be'
  on-tertiary-container: '#333b4f'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#ffdea5'
  primary-fixed-dim: '#e9c176'
  on-primary-fixed: '#261900'
  on-primary-fixed-variant: '#5d4201'
  secondary-fixed: '#b7eaff'
  secondary-fixed-dim: '#4cd6ff'
  on-secondary-fixed: '#001f28'
  on-secondary-fixed-variant: '#004e60'
  tertiary-fixed: '#dae2fd'
  tertiary-fixed-dim: '#bec6e0'
  on-tertiary-fixed: '#131b2e'
  on-tertiary-fixed-variant: '#3f465c'
  background: '#10131a'
  on-background: '#e1e2eb'
  surface-variant: '#32353c'
  brass-gold: '#C5A059'
  cyber-blue: '#00D1FF'
  parchment-text: '#E2E8F0'
  deep-navy: '#0B0E14'
  ink-black: '#05070A'
typography:
  headline-display:
    fontFamily: EB Garamond
    fontSize: 48px
    fontWeight: '600'
    lineHeight: 56px
    letterSpacing: -0.02em
  headline-lg:
    fontFamily: EB Garamond
    fontSize: 32px
    fontWeight: '500'
    lineHeight: 40px
  headline-lg-mobile:
    fontFamily: EB Garamond
    fontSize: 28px
    fontWeight: '500'
    lineHeight: 36px
  body-md:
    fontFamily: Hanken Grotesk
    fontSize: 16px
    fontWeight: '400'
    lineHeight: 24px
  body-sm:
    fontFamily: Hanken Grotesk
    fontSize: 14px
    fontWeight: '400'
    lineHeight: 20px
  label-caps:
    fontFamily: JetBrains Mono
    fontSize: 12px
    fontWeight: '600'
    lineHeight: 16px
    letterSpacing: 0.1em
  data-mono:
    fontFamily: JetBrains Mono
    fontSize: 13px
    fontWeight: '400'
    lineHeight: 18px
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  unit: 8px
  gutter: 24px
  margin-mobile: 16px
  margin-desktop: 64px
  container-max: 1280px
---

## Brand & Style

The design system embodies the "Data-driven Trust" narrative, a synthesis of historical Danish administrative authority and cutting-edge artificial intelligence. It is designed for Danish micro-businesses who require the reliability of a traditional bookkeeper with the speed of a modern SaaS.

The aesthetic direction is **Modern-Traditionalist**. It utilizes heavy, atmospheric background tones reminiscent of 18th-century "Rentekammer" offices, punctuated by precise, luminous UI elements that represent AI logic. The style balances the warmth of brass and parchment with the cold, clinical precision of neon data grids. This contrast ensures the product feels both established and futuristic.

## Colors

The palette is anchored in a **dark-mode default** to evoke the atmosphere of a dimly lit, historic study.

- **Primary (Brass Gold):** Used for primary actions, high-level headings, and signals of "Official" status. It reflects the weight of tradition and value.
- **Secondary (Cyber Blue):** Reserved for AI-driven insights, data visualizations, and highlights. This color represents the "digital pulse" of the system.
- **Neutral (Deep Navy/Ink):** Used for the canvas and surface layers. These are near-black tones with a slight blue-green tint to provide depth and sophistication.
- **Text (Parchment):** Avoid pure white. Use a slightly warm, desaturated off-white for body text to reduce eye strain and maintain the historical theme.

## Typography

This design system uses a triple-font strategy to differentiate between **Narrative (Serif)**, **UI/Reading (Sans)**, and **Logic (Mono)**.

- **EB Garamond** (Headlines): Used for page titles and section headers. It provides the "institutional trust" required for an accounting tool.
- **Hanken Grotesk** (Interface & Body): A sharp, contemporary sans-serif used for all functional text and inputs. It ensures high legibility for financial data.
- **JetBrains Mono** (Labels & Data): Used for secondary labels, table data, and AI-generated code/scripts. Its technical appearance reinforces the precision of the AI.

Always use serif fonts for "Status" or "Legacy" headings and sans-serif for "Action" and "Input" components.

## Layout & Spacing

The layout follows a **Fixed Grid** philosophy for desktop to maintain a sense of a "contained ledger," while transitioning to a fluid model for mobile.

- **Grid Model:** A 12-column grid with 24px gutters. Use generous outer margins (64px+) on desktop to create a premium, editorial feel with significant whitespace.
- **Vertical Rhythm:** Based on an 8px baseline. All components and spacing increments must be multiples of 8.
- **Information Density:** High density for data tables (ledgers), but low density for dashboard overviews and setup screens.
- **Breakpoints:**
  - Mobile: < 768px (4 columns, 16px margins)
  - Tablet: 768px - 1024px (8 columns, 32px margins)
  - Desktop: > 1024px (12 columns, 64px margins)

## Elevation & Depth

Hierarchy is established through **Tonal Layers** and **Cyber-Blue Glows** rather than traditional drop shadows.

- **Surface Layers:** The base background is `ink-black`. Containers use `deep-navy` to appear slightly raised. A subtle 1px border in a desaturated brass or dark blue should be used to define boundaries.
- **AI Glow:** Interactive AI elements or active data nodes should use a soft, inner-glow effect (0px blur, 4px spread) using a low-opacity `cyber-blue`.
- **Glassmorphism:** Use sparingly for overlay modals and dropdown menus. A subtle backdrop-blur (12px) with a semi-transparent `deep-navy` background mimics the look of a digital lens over physical parchment.
- **Depth markers:** Use thin, horizontal "brass" lines (1px) to separate major sections, referencing the lines of a traditional ledger.

## Shapes

The shape language is **Structured and Precise**. 

We use **Soft (0.25rem)** roundedness for standard UI elements like input fields and buttons. This avoids the "playfulness" of highly rounded corners while preventing the "harshness" of sharp edges.

- **Buttons:** 4px radius (`rounded-sm`).
- **Cards/Containers:** 8px radius (`rounded-lg`).
- **Data Tags/Chips:** 2px radius or sharp edges to emphasize a "microchip" or "ledger cell" aesthetic.
- **Dividers:** Use custom "Brass" fleurons or simple 1px lines to break content, adding a touch of 18th-century ornamentation.

## Components

- **Buttons:**
  - *Primary:* Brass-gold background, ink-black text. Bold and authoritative.
  - *Secondary:* Ghost style with cyber-blue borders and text. Used for AI-related actions.
- **Inputs:** Dark backgrounds with 1px brass-gold borders on focus. Labels should use `label-caps` in JetBrains Mono.
- **Cards:** Subtle `deep-navy` surfaces with 1px low-opacity borders. No heavy shadows; use a slight outer glow of `cyber-blue` if the card is "AI-active."
- **Data Tables (Ledgers):** The core of the product. Alternate row colors using `ink-black` and `deep-navy`. Header text should be `label-caps`. Use vertical lines sparingly to maintain a clean look.
- **AI Insights (Chips/Toasts):** Use the `cyber-blue` for icons and borders to signal that the information was generated by the AI agent.
- **Progress Bars:** Use a "neon-grid" texture within the progress bar, utilizing the `cyber-blue` highlight color.