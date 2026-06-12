/**
 * CLI commands for `invoice <subcommand>`.
 *
 * Implementation note: the 25 subcommand registrations used to live inline in
 * this file (809 lines). They were split into per-sub-domain helpers in
 * `./invoice/`. Each helper is called below in the same order the subcommands
 * were originally registered — `--help` output and any test that walks the
 * dispatch table relies on this sequence.
 */

import type { CommandDispatch } from "../cli-dispatch";
import { registerIssuanceCommands } from "./invoice/issuance";
import { registerSettlementCommands } from "./invoice/settlement";
import { registerReminderCommands } from "./invoice/reminder";
import { registerQueryCommands } from "./invoice/query";
import { registerInterestCommands } from "./invoice/interest";
import { registerCompensationCommands } from "./invoice/compensation";

export function register(dispatch: CommandDispatch): void {
  // Order is load-bearing — see the file-level comment.
  registerIssuanceCommands(dispatch);
  registerSettlementCommands(dispatch);
  registerReminderCommands(dispatch);
  registerQueryCommands(dispatch);
  registerInterestCommands(dispatch);
  registerCompensationCommands(dispatch);
}
