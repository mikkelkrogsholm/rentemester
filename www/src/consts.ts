export const SITE = {
  url: "https://rentemester.dk",
  name: "Rentemester",
  tagline: "Open source bogføring til danske virksomheder",
  description:
    "Rentemester er et open source projekt: et agent-first bogføringssystem til danske mikrovirksomheder, freelancere og små ApS'er. Drevet af danske regler, append-only ledger og kryptografisk verifikation. MIT-licens.",
  locale: "da_DK",
  lang: "da",
  github: "https://github.com/mikkelkrogsholm/rentemester",
  githubRaw: "github.com/mikkelkrogsholm/rentemester",
  email: "kontakt@rentemester.dk",
  author: "Mikkel Krogsholm",
  license: "MIT",
} as const;

export const NAV = [
  { href: "/funktioner", label: "Funktioner" },
  { href: "/sådan-virker-det", label: "Sådan virker det" },
  { href: "/hvorfor", label: "Hvorfor" },
  { href: "/docs/installation", label: "Dokumentation" },
] as const;
