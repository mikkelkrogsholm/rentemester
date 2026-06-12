// #347 — Lovgrundlag-viewer (read-only).

export type RuleBundleSummary = {
  name: string;
  version: string;
  ruleCount: number;
  sources: string[];
  vatCodes: string[];
};

export type RuleProvisionCitation = {
  ref: string;
  textHash: string;
};

export type RuleSummary = {
  ruleId: string;
  bundle: string;
  sourceId: string;
  name: string;
  explanation: string;
  severity: string;
  category: string;
  provisions: RuleProvisionCitation[];
};

export type LegalSource = {
  id: string;
  title: string;
  authority: string;
  category: string;
  url: string;
  xmlUrl?: string;
  notes?: string;
};

export type RulesResponse = {
  ok: true;
  ruleBundles: RuleBundleSummary[];
  rules: RuleSummary[];
  legalSources: LegalSource[];
};
