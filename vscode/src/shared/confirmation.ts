// The judgement of `free` and `fg` on a row (X12) and the words of their dialogs; host/confirm.ts
// shows them. Both refuse while an actor in the slot works or waits (for `fg`, the slot that
// holds the branch: the row itself), ask with a warning when one is idle (quiet counts as idle),
// and judge the row again, from the latest document, at the answer.
import type { Worktree } from '../generated/status';
import { plain } from './invisible';
import { holdsBranch, type BranchSlot } from './protocol';

/** The row actions that ask in a modal dialog before they run (X12). */
export type ConfirmedActionType = 'free' | 'fg';

/** Holds the modal question to ask before a `free` or `fg`. */
export interface Question {
  /** True while an agent in the slot is idle: the dialog warns instead of asking plainly. */
  warning: boolean;
  message: string;
  detail: string;
  /** The label of the button that goes ahead. */
  confirm: string;
}

/** Tells a worktree where an actor works or waits, the cases where `free` and `fg` refuse. */
function busy(worktree: Worktree): boolean {
  return worktree.actors.some((actor) => actor.state === 'working' || actor.state === 'waiting');
}

/** Tells a worktree where an actor is idle or quiet, the cases where the dialog warns. */
function idle(worktree: Worktree): boolean {
  return worktree.actors.some((actor) => actor.state === 'idle' || actor.state === 'quiet');
}

/**
 * Gives the error text of a `free` or `fg` that refuses, or undefined when no agent in the slot
 * works or waits.
 */
export function refusal(type: ConfirmedActionType, worktree: BranchSlot): string | undefined {
  if (!busy(worktree)) {
    return undefined;
  }
  const name = worktree.name;
  return type === 'free'
    ? `${name} is not freed, an agent there works or waits. Free it once the agent is idle.`
    : `${worktree.branch} is not brought to the foreground, an agent in ${name} works or waits. Try again once the agent is idle.`;
}

/** Gives the question to ask before `type` on `worktree`, naming the worktree and the branch. */
export function question(type: ConfirmedActionType, worktree: BranchSlot): Question {
  const name = plain(worktree.name);
  const branch = plain(worktree.branch);
  const warning = idle(worktree);
  if (type === 'free') {
    return {
      warning,
      message: warning
        ? `An agent in ${name} is idle. Free ${name} from branch ${branch} anyway?`
        : `Free ${name} from branch ${branch}?`,
      detail: `wtpool free ${name} detaches ${name} from ${branch}. The branch stays.`,
      confirm: 'Free',
    };
  }
  return {
    warning,
    message: warning
      ? `An agent in ${name} is idle. Bring branch ${branch} from ${name} to the foreground anyway?`
      : `Bring branch ${branch} from ${name} to the foreground?`,
    detail: `wtpool fg ${branch} detaches ${name} and checks out ${branch} in the foreground worktree.`,
    confirm: 'Bring to Foreground',
  };
}

/**
 * Judges the row again at the answer: tells a row `now`, the latest state of `asked`, that still
 * holds the branch the user was asked about.
 */
export function stillHolds(asked: BranchSlot, now: Worktree | undefined): now is BranchSlot {
  return now !== undefined && holdsBranch(now) && now.branch === asked.branch;
}

/** Gives the error text for a row that changed while the dialog was open. */
export function changedText(asked: Worktree): string {
  return `${asked.name} changed while you were asked, so nothing ran.`;
}

/** Gives the arguments of the `wtpool` that `type` runs: `free <worktree>` or `fg <branch>`. */
export function wtpoolArgs(type: ConfirmedActionType, worktree: BranchSlot): string[] {
  return type === 'free' ? ['free', worktree.name] : ['fg', worktree.branch];
}
