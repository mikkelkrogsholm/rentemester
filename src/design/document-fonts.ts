export const documentFonts: ReadonlyArray<{ name: string; weight: number; file: string; subsetOf?: string }> = [
  { name: "IBM Plex Sans", weight: 400, file: "ibm-plex-sans-latin-400-normal.woff2" },
  { name: "IBM Plex Sans", weight: 600, file: "ibm-plex-sans-latin-600-normal.woff2" },
  { name: "IBM Plex Mono", weight: 400, file: "ibm-plex-mono-latin-400-normal.woff2" },
  { name: "Source Serif 4", weight: 600, file: "source-serif-4-latin-600-normal.woff2" },
  { name: "IBM Plex Sans Latin Extended", subsetOf: "IBM Plex Sans", weight: 400, file: "ibm-plex-sans-latin-ext-400-normal.woff2" },
  { name: "IBM Plex Sans Latin Extended", subsetOf: "IBM Plex Sans", weight: 600, file: "ibm-plex-sans-latin-ext-600-normal.woff2" },
  { name: "IBM Plex Mono Latin Extended", subsetOf: "IBM Plex Mono", weight: 400, file: "ibm-plex-mono-latin-ext-400-normal.woff2" },
  { name: "Source Serif 4 Latin Extended", subsetOf: "Source Serif 4", weight: 600, file: "source-serif-4-latin-ext-600-normal.woff2" },
];
