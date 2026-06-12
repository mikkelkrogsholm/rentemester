export type LinkItem = {
  href: string;
  label: string;
  external?: boolean;
};

export type LinkGroup = {
  title: string;
  links: readonly LinkItem[];
};

export const MAIN_NAV = [
  { href: "/funktioner", label: "Funktioner" },
  { href: "/regnskabsprogram", label: "Regnskabsprogram" },
  { href: "/viden", label: "Viden" },
  { href: "/vaerktoej/momsberegner", label: "Momsberegner" },
  { href: "/saadan-virker-det", label: "Sådan virker det" },
] as const satisfies readonly LinkItem[];

export const VIDEN_SECTIONS = [
  {
    title: "Målgrupper",
    links: [
      { href: "/bogfoering-for-freelancere", label: "Bogføring for freelancere" },
      { href: "/bogfoering-for-enkeltmandsvirksomhed", label: "Bogføring for enkeltmandsvirksomhed" },
      { href: "/bogfoering-for-aps", label: "Bogføring for ApS" },
      { href: "/open-source-regnskabsprogram", label: "Open source-regnskabsprogram" },
    ],
  },
  {
    title: "Moms",
    links: [
      { href: "/viden/moms/satser", label: "Momssatser i Danmark" },
      { href: "/viden/frister/moms", label: "Momsfrister" },
      { href: "/viden/moms/eu-reverse-charge", label: "EU reverse charge" },
      { href: "/viden/moms/vies-momsnummer", label: "VIES og momsnummer" },
      { href: "/viden/moms/momsrapport", label: "Momsrapport" },
      { href: "/viden/moms/momsindberetning", label: "Momsindberetning" },
    ],
  },
  {
    title: "Sådan bogfører du",
    links: [
      { href: "/viden/saadan-bogfoerer-du/faktura", label: "Faktura" },
      { href: "/viden/saadan-bogfoerer-du/leverandoerfaktura", label: "Leverandørfaktura" },
      { href: "/viden/saadan-bogfoerer-du/kreditnota", label: "Kreditnota" },
      { href: "/viden/saadan-bogfoerer-du/bankafstemning", label: "Bankafstemning" },
      { href: "/viden/saadan-bogfoerer-du/banktransaktion", label: "Banktransaktion" },
      { href: "/viden/saadan-bogfoerer-du/bilag", label: "Bilag" },
      { href: "/viden/saadan-bogfoerer-du/kvittering", label: "Kvittering" },
      { href: "/viden/saadan-bogfoerer-du/kassebon", label: "Kassebon" },
      { href: "/viden/saadan-bogfoerer-du/udlaeg", label: "Udlæg" },
      { href: "/viden/saadan-bogfoerer-du/mobilepay", label: "MobilePay-betalinger" },
      { href: "/viden/saadan-bogfoerer-du/repraesentation", label: "Repræsentation" },
      { href: "/viden/saadan-bogfoerer-du/koerselsgodtgorelse", label: "Kørselsgodtgørelse" },
      { href: "/viden/saadan-bogfoerer-du/udenlandsk-faktura", label: "Udenlandsk faktura" },
      { href: "/viden/saadan-bogfoerer-du/valuta", label: "Valuta" },
    ],
  },
  {
    title: "Debitorer",
    links: [
      { href: "/viden/fakturering/e-faktura-offentlig", label: "Offentlig e-faktura" },
      { href: "/viden/fakturering/gentagne-fakturaer", label: "Gentagne fakturaer" },
      { href: "/viden/debitorer/rykkergebyr-morarente", label: "Rykkergebyr og morarente" },
      { href: "/viden/debitorer/tab-paa-debitorer", label: "Tab på debitorer" },
      { href: "/viden/debitorer/faktura-status", label: "Fakturastatus" },
      { href: "/viden/debitorer/refundering", label: "Refundering" },
    ],
  },
  {
    title: "Regler og begreber",
    links: [
      { href: "/viden/digital-bogfoering", label: "Digital bogføring" },
      { href: "/viden/bogfoeringsloven", label: "Bogføringsloven" },
      { href: "/viden/begreber/afstemning", label: "Afstemning" },
      { href: "/viden/begreber/audit-trail", label: "Audit trail" },
      { href: "/viden/begreber/bilagsnummer", label: "Bilagsnummer" },
      { href: "/viden/begreber/append-only-ledger", label: "Append-only ledger" },
      { href: "/viden/begreber/debet-kredit", label: "Debet og kredit" },
      { href: "/viden/begreber/kontoplan", label: "Kontoplan" },
      { href: "/viden/regnskab/aabningsbalance", label: "Åbningsbalance" },
      { href: "/viden/regnskab/aarsrapport", label: "Årsrapport og iXBRL" },
      { href: "/viden/regnskab/flere-virksomheder", label: "Flere virksomheder" },
      { href: "/viden/regnskab/kreditorer", label: "Kreditorer" },
      { href: "/viden/regnskab/periodeafgraensning", label: "Periodeafgrænsning" },
      { href: "/viden/regnskab/budget-likviditet", label: "Budget og likviditet" },
      { href: "/viden/regnskab/skatteopgoerelse", label: "Skatteopgørelse" },
      { href: "/viden/anlaegsaktiver/afskrivning", label: "Anlægsaktiver og afskrivning" },
    ],
  },
  {
    title: "Sikkerhed og handoff",
    links: [
      { href: "/viden/sikkerhed/backup-af-regnskab", label: "Backup af regnskab" },
      { href: "/viden/sikkerhed/signeret-backup", label: "Signeret backup" },
      { href: "/viden/sikkerhed/lokal-regnskabsdata", label: "Lokal regnskabsdata" },
      { href: "/viden/sikkerhed/gdpr", label: "GDPR i bogføring" },
      { href: "/viden/myndigheder/eksport-af-regnskab", label: "Eksport af regnskab" },
      { href: "/viden/myndigheder/saft", label: "SAF-T eksport" },
      { href: "/viden/myndigheder/regnskabsmateriale", label: "Regnskabsmateriale" },
      { href: "/viden/myndigheder/revisor-handoff", label: "Revisor-handoff" },
    ],
  },
  {
    title: "Import og stamdata",
    links: [
      { href: "/viden/import/dinero", label: "Import fra Dinero" },
      { href: "/viden/stamdata/cvr", label: "CVR og stamdata" },
      { href: "/viden/bilag/bilagsmail", label: "Bilagsmail" },
    ],
  },
  {
    title: "AI og kontrol",
    links: [
      { href: "/ai-bogholder", label: "AI-bogholder" },
      { href: "/viden/ai/ai-bogfoering-med-kontrol", label: "AI-bogføring med kontrol" },
      { href: "/viden/ai/mcp-bogfoering", label: "MCP til bogføring" },
    ],
  },
  {
    title: "Værktøjer",
    links: [
      { href: "/vaerktoej/momsberegner", label: "Momsberegner" },
      { href: "/vaerktoej/fakturaskabelon", label: "Fakturaskabelon" },
    ],
  },
] as const satisfies readonly LinkGroup[];

export const FOOTER_PROJECT_LINKS = [
  { href: "/regnskabsprogram", label: "Regnskabsprogram" },
  { href: "/open-source-regnskabsprogram", label: "Open source-regnskab" },
  { href: "/bogfoeringsprogram", label: "Bogføringsprogram" },
  { href: "/ai-bogholder", label: "AI-bogholder" },
  { href: "/hvorfor", label: "Hvorfor" },
  { href: "/saadan-virker-det", label: "Sådan virker det" },
  { href: "/funktioner", label: "Funktioner" },
  { href: "/om", label: "Om" },
  { href: "/about", label: "About" },
  { href: "/kontakt", label: "Kontakt" },
] as const satisfies readonly LinkItem[];

export const FOOTER_KNOWLEDGE_LINKS = [
  { href: "/viden", label: "Videnshub" },
  ...VIDEN_SECTIONS.flatMap((section) => section.links),
  { href: "/docs/installation", label: "Installation" },
  { href: "/sikkerhed", label: "Sikkerhed" },
] as const satisfies readonly LinkItem[];

export const FOOTER_LEGAL_LINKS = [
  { href: "/privacy-policy", label: "Privacy Policy" },
  { href: "/privatlivspolitik", label: "Privatliv" },
  { href: "/sikkerhed", label: "Sikkerhed" },
  { href: "/kontakt", label: "Kontakt" },
  { href: "/sitemap-index.xml", label: "Sitemap" },
] as const satisfies readonly LinkItem[];

const SECTION_LABELS: Record<string, string> = {
  ai: "AI",
  anlaegsaktiver: "Anlægsaktiver",
  begreber: "Begreber",
  bilag: "Bilag",
  debitorer: "Debitorer",
  docs: "Dokumentation",
  fakturering: "Fakturering",
  frister: "Frister",
  import: "Import",
  moms: "Moms",
  myndigheder: "Myndigheder",
  regnskab: "Regnskab",
  "saadan-bogfoerer-du": "Sådan bogfører du",
  sikkerhed: "Sikkerhed",
  stamdata: "Stamdata",
  vaerktoej: "Værktøj",
  viden: "Viden",
};

export const ROUTE_LABELS: Record<string, string> = {
  "/": "Forside",
  "/404": "Siden findes ikke",
  ...Object.fromEntries([
    ...MAIN_NAV,
    ...FOOTER_PROJECT_LINKS,
    ...FOOTER_KNOWLEDGE_LINKS,
    ...FOOTER_LEGAL_LINKS,
  ].map((link) => [link.href, link.label])),
};

export const labelForPath = (path: string): string => {
  const normalizedPath = path === "/" ? "/" : path.replace(/\/$/, "");
  const exactLabel = ROUTE_LABELS[normalizedPath];
  if (exactLabel) return exactLabel;

  const lastSegment = normalizedPath.split("/").filter(Boolean).at(-1) ?? "";
  return SECTION_LABELS[lastSegment] ?? lastSegment.replace(/-/g, " ");
};
