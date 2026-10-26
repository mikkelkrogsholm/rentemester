import { describe, expect, test } from "bun:test";
import { finalizeVatForm } from "../../src/core/vat-rubric";

const zero = {
  salgsmoms: 0, kobsmoms: 0, momsAfVarekobUdland: 0, momsAfYdelseskobUdland: 0,
  rubrikAVarer: 0, rubrikAYdelser: 0, rubrikBVarerEuSalesList: 0,
  rubrikBVarerIkkeEuSalesList: 0, rubrikBYdelser: 0, rubrikC: 0,
  olieOgFlaskegasafgift: 0, elafgift: 0, naturgasOgBygasafgift: 0,
  kulafgift: 0, co2Afgift: 0, vandafgift: 0,
};

describe("exact TastSelv VAT form (#607)", () => {
  test("truncates every submitted field before calculating the statutory total", () => {
    const form = finalizeVatForm({ ...zero, salgsmoms: 4457.25, momsAfYdelseskobUdland: 375.56, kobsmoms: 4528.86 }, 303.95);
    expect(form).toMatchObject({ salgsmoms: 4457, momsAfYdelseskobUdland: 375, kobsmoms: 4528, momsIAlt: 304 });
    expect(form.wholeKronerDifferenceDkk).toBeCloseTo(0.05, 8);
  });

  test("keeps A/B/C total-neutral and handles signed corrections towards zero", () => {
    const form = finalizeVatForm({ ...zero, salgsmoms: -25.99, rubrikAVarer: 8814.74, rubrikAYdelser: 1449.41, rubrikBVarerEuSalesList: 10.99, rubrikBVarerIkkeEuSalesList: 20.99, rubrikBYdelser: 30.99, rubrikC: 40.99 });
    expect(form).toMatchObject({ salgsmoms: -25, rubrikAVarer: 8814, rubrikAYdelser: 1449, rubrikBVarerEuSalesList: 10, rubrikBVarerIkkeEuSalesList: 20, rubrikBYdelser: 30, rubrikC: 40, momsIAlt: -25 });
  });

  test("truncates each refund independently before reducing the total", () => {
    const form = finalizeVatForm({ ...zero, salgsmoms: 100, olieOgFlaskegasafgift: 1.99, elafgift: 1.99, vandafgift: -2.99 });
    expect(form).toMatchObject({ olieOgFlaskegasafgift: 1, elafgift: 1, vandafgift: -2, momsIAlt: 100 });
  });
});
