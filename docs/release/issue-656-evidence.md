# #656 candidate evidence profile

This issue remains open until the final candidate image has browser screenshots.

| Surface | Candidate-visible evidence | Safe keyboard task |
| --- | --- | --- |
| `/companies/:slug/moms` | `Indberetning`, one of `Ikke klar`, `Kræver stillingtagen`, `Klar`, `Lukket/endelig`, amount, deadline, `Kopiér alle SKAT-felter` | Tab to `Kopiér alle SKAT-felter`, press Enter, and verify the clipboard contains deterministic `label;whole-kroner` rows. This does not submit anything. |
| `/companies/:slug/aarsrapport` | canonical selected-year dates, `Readiness` list and disabled `Byg årsrapport` while not green | Select a fiscal year with the keyboard and verify that the build button remains disabled when readiness is not green. |
| `/companies/:slug/periodisering` | `Sikker næste handling`, attention link and `Kopiér sikker næste handling` | Tab to the copy button, press Enter, and verify the copied instruction only routes to review/dry-run; it does not create or recognize an accrual. |
| `GET /api/companies/:slug/periods/close-readiness` | `packet` plus non-durable `readiness` projection | Confirm it is a GET/read permission route and that the packet hash is unchanged by the projection. |

The public agent equivalents are `period readiness` and MCP `period_close_readiness`; both return the same read-only `readiness` projection beside the packet. Review/close remains a separate confirmed, actor-attributed workflow with review ID and packet hash.
