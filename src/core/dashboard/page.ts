// Outer `renderDashboard()` composer — assembles the page from the
// per-section render functions.

import { escapeHtml, type DashboardInput, type RenderOptions } from "./_shared";
import { buildStyle, fontLink } from "./styles";
import { header } from "./header";
import { metricsSection } from "./metrics";
import { exceptionsSection } from "./exceptions";
import { deadlineSection } from "./deadline";
import { invoiceTable } from "./invoices";
import { payablesSection } from "./payables";
import { accrualsSection } from "./accruals";
import { budgetLiquiditySection } from "./budget";
import { taxSection } from "./tax";
import { euSalesOssSection, hasEuSalesOssActivity } from "./eu-sales-oss";
import { activityList } from "./activity";
import { statusSection } from "./status";
import { footer } from "./footer";

// --------------------------------------------------------------------------
// Main render
// --------------------------------------------------------------------------

export function renderDashboard(input: DashboardInput, _options: RenderOptions = {}): string {
  const company = input.company;
  const title = `Rentemester — ${company.name} — ${input.asOfDate}`;

  const sections = [
    header(input),
    metricsSection(input),
    `<section class="section"><h2>Åbne exceptions</h2>${exceptionsSection(input)}</section>`,
    `<section class="section"><h2>Næste deadline</h2>${deadlineSection(input)}</section>`,
    `<section class="section"><h2>Åbne fakturaer</h2>${invoiceTable(input.invoices)}</section>`,
  ];

  // Creditor card — symmetric to the open-invoices (debitor) view above.
  if (input.payables) {
    sections.push(
      `<section class="section"><h2>Åbne kreditorposter</h2>${payablesSection(input.payables)}</section>`,
    );
  }
  // Accruals — open balance-sheet exposure + due recognition periods.
  if (input.accrualRegister || input.accrualsDue) {
    sections.push(
      `<section class="section"><h2>Periodeafgrænsningsposter</h2>${accrualsSection(input.accrualRegister, input.accrualsDue)}</section>`,
    );
  }
  // Budget & liquidity — budget-vs-actual + the forward forecast.
  if (input.budgetVsActual || input.liquidity) {
    sections.push(
      `<section class="section"><h2>Budget &amp; likviditet</h2>${budgetLiquiditySection(input.budgetVsActual, input.liquidity)}</section>`,
    );
  }
  // Tax — estimated selskabsskat, or the "awaiting year-end" state.
  if (input.tax) {
    sections.push(
      `<section class="section"><h2>Skat</h2>${taxSection(input.tax)}</section>`,
    );
  }
  // EU sales / OSS — a light indicator: only surfaced when there is activity
  // that needs a separate filing.
  if (hasEuSalesOssActivity(input.euSalesOss)) {
    sections.push(
      `<section class="section"><h2>EU-salg &amp; OSS</h2>${euSalesOssSection(input.euSalesOss!)}</section>`,
    );
  }

  sections.push(
    `<section class="section"><h2>Seneste aktivitet</h2>${activityList(input.recentActivity)}</section>`,
    statusSection(input),
    footer(input),
  );

  const rendered = sections.join("\n");

  return `<!DOCTYPE html>
<html lang="da">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(title)}</title>
${fontLink()}
<style>
${buildStyle()}
</style>
</head>
<body>
<main class="page">
${rendered}
</main>
</body>
</html>
`;
}
