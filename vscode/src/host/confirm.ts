// `free` and `fg` from a row (X12): each asks the question of shared/confirmation.ts in a modal
// dialog and runs `wtpool free <worktree>` or `wtpool fg <branch>` only on confirm, after judging
// the row again from the latest document.
import * as vscode from 'vscode';

import type { Worktree } from '../generated/status';
import {
  changedText,
  question,
  refusal,
  stillHolds,
  wtpoolArgs,
  type ConfirmedActionType,
  type Question,
} from '../shared/confirmation';
import type { BranchSlot } from '../shared/protocol';
import { showError } from './actions';
import { describeExit, runToEnd, type RunEnd } from './spawn';

/** The most characters of wtpool's error a notification shows; the log keeps all of it. */
const SHOWN_REASON_CHARS = 300;

/**
 * Gives the confirm step its view of the tab: the latest state of a worktree, the `wtpool`
 * binary, and the folder `wtpool` runs in, the tab's workspace folder (R9).
 */
export interface ConfirmContext {
  latest(path: string): Worktree | undefined;
  binary(): string;
  cwd(): string | undefined;
  log(message: string): void;
}

/** Asks `asked` in a modal dialog, a warning or a plain question, and tells a confirm. */
async function ask(asked: Question): Promise<boolean> {
  const options = { modal: true, detail: asked.detail };
  const answer = asked.warning
    ? await vscode.window.showWarningMessage(asked.message, options, asked.confirm)
    : await vscode.window.showInformationMessage(asked.message, options, asked.confirm);
  return answer === asked.confirm;
}

/**
 * Describes a run that failed: the reason it did not start, or the text wtpool printed, else its
 * exit; gives undefined for a run that succeeded.
 */
function failureOf(end: RunEnd): string | undefined {
  if (!end.started) {
    return end.error.message;
  }
  if (end.code === 0) {
    return undefined;
  }
  return end.stderr.trim() || describeExit(end.code, end.signal);
}

/** Runs `wtpool <args>` in the tab's folder (R9), and logs and shows an error when it fails. */
async function runWtpool(context: ConfirmContext, args: string[]): Promise<void> {
  const reason = failureOf(await runToEnd(context.binary(), args, context.cwd()));
  if (reason === undefined) {
    return;
  }
  const command = `wtpool ${args.join(' ')}`;
  context.log(`${command}: ${reason}`);
  showError(`${command} failed: ${reason.slice(0, SHOWN_REASON_CHARS)}`);
}

/** Asks, judges again at the answer, and runs: the whole of a `free` or `fg` (X12). */
export async function confirm(
  context: ConfirmContext,
  type: ConfirmedActionType,
  asked: BranchSlot,
): Promise<void> {
  const refused = refusal(type, asked);
  if (refused !== undefined) {
    showError(refused);
    return;
  }
  if (!(await ask(question(type, asked)))) {
    return;
  }
  // The pool may have moved on while the dialog was open: judge the row as it is now.
  const now = context.latest(asked.path);
  if (!stillHolds(asked, now)) {
    showError(changedText(asked));
    return;
  }
  const refusedNow = refusal(type, now);
  if (refusedNow !== undefined) {
    showError(refusedNow);
    return;
  }
  await runWtpool(context, wtpoolArgs(type, now));
}
