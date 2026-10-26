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

/** The most characters of wtpool's error a notification shows. The log keeps all of it. */
const SHOWN_REASON_CHARS = 300;

/** The parts of a tab that {@link confirm} uses. */
export interface ConfirmContext {
  /** Gives the worktree at `path` in the tab's latest document. */
  latest(path: string): Worktree | undefined;
  binary(): string;
  cwd(): string | undefined;
  log(message: string): void;
}

async function ask(asked: Question): Promise<boolean> {
  const options = { modal: true, detail: asked.detail };
  const answer = asked.warning
    ? await vscode.window.showWarningMessage(asked.message, options, asked.confirm)
    : await vscode.window.showInformationMessage(asked.message, options, asked.confirm);
  return answer === asked.confirm;
}

function failureOf(end: RunEnd): string | undefined {
  if (!end.started) {
    return end.error.message;
  }
  if (end.code === 0) {
    return undefined;
  }
  return end.stderr.trim() || describeExit(end.code, end.signal);
}

async function runWtpool(context: ConfirmContext, args: string[]): Promise<void> {
  const reason = failureOf(await runToEnd(context.binary(), args, context.cwd()));
  if (reason === undefined) {
    return;
  }
  const command = `wtpool ${args.join(' ')}`;
  context.log(`${command}: ${reason}`);
  showError(`${command} failed: ${reason.slice(0, SHOWN_REASON_CHARS)}`);
}

/**
 * Runs `free` or `fg` on `asked` once the user confirms it in a modal dialog. A refusal, or a row
 * that changed while the dialog was open, shows an error and runs nothing.
 */
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
