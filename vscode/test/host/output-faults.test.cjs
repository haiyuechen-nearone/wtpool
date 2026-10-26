// Output tails and Follow in the extension host when the system gets in the way of a read. A
// watch that cannot be made, or that fails, leaves the 1 s poll to see the file grow. A change
// made while a read holds the file is read after that read, never by a second read at the same
// time. A followed file replaced by a symlink shows nothing more.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const { sleep, waitFor, wrap } = require('./support.cjs');
const { QUIET_MS, WT2, actor, lines, nextOutput, output, outputFolder, outputs, poolWith, start } = require('./outputs.cjs');

/** The longest a check waits for the poll to see a change: its 1 s, a read and a post. */
const POLL_WAIT_MS = 3_000;
/** The time each read of a file in the check's folder holds the file open before it closes. */
const SLOW_CLOSE_MS = 600;

/** Makes `fs.watch` throw for each path below `dir`, as a system out of watches does. */
function failWatches(dir) {
  return wrap(
    fs,
    'watch',
    (target) => !String(target).startsWith(dir),
    () => {
      throw Object.assign(new Error('EMFILE: too many open files, watch'), { code: 'EMFILE' });
    },
  );
}

/** Keeps each watcher `fs.watch` makes for a path below `dir`, so a check can make it fail. */
function keepWatchers(dir) {
  const original = fs.watch;
  const made = [];
  fs.watch = function keptWatch(target, ...rest) {
    const watcher = original.call(this, target, ...rest);
    if (String(target).startsWith(dir)) made.push(watcher);
    return watcher;
  };
  return {
    made,
    restore() {
      fs.watch = original;
    },
  };
}

/**
 * Holds each file below `dir` that the extension opens for {@link SLOW_CLOSE_MS} before it
 * closes, and counts the reads of one file under way at once.
 */
function slowReads(dir) {
  const original = fs.promises.open;
  const open = new Map();
  let most = 0;
  fs.promises.open = async function slowOpen(target, ...rest) {
    const handle = await original.call(this, target, ...rest);
    if (!String(target).startsWith(dir)) return handle;
    open.set(target, (open.get(target) ?? 0) + 1);
    most = Math.max(most, open.get(target));
    const close = handle.close.bind(handle);
    handle.close = async () => {
      await sleep(SLOW_CLOSE_MS);
      open.set(target, open.get(target) - 1);
      return close();
    };
    return handle;
  };
  return {
    most: () => most,
    restore() {
      fs.promises.open = original;
    },
  };
}

/** Opens the Follow terminal of `file`, listed for wt2, and collects what it writes. */
async function followed(file, terminals) {
  const api = await start(poolWith({ wt2: [actor('s2', [output(file)])] }));
  await api.fromWebview({ type: 'follow', id: file });
  const options = await waitFor(() => terminals.calls[0]?.[0], 5_000);
  assert.ok(options?.pty, 'a follow opened no terminal with a Pseudoterminal');
  const written = [];
  options.pty.onDidWrite((data) => written.push(data));
  options.pty.open(undefined);
  return { pty: options.pty, text: () => written.join('') };
}

function count(text, part) {
  return text.split(part).length - 1;
}

describe('output reads against a system that gets in the way', () => {
  let undo = [];
  afterEach(async () => {
    for (const one of undo.reverse()) one.restore();
    undo = [];
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
  });

  /** Opens wt2's tail of `file` and waits for its first output message. */
  async function selected(file) {
    const api = await start(poolWith({ wt2: [actor('s2', [output(file)])] }));
    const before = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    assert.ok(await nextOutput(before, WT2), 'selecting wt2 posted no output message');
  }

  /** The output message after `before` whose tail of wt2 ends with `line`, within the poll's wait. */
  function shown(before, line) {
    return nextOutput(before, WT2, (m) => m.tails[0].lines.at(-1) === line, POLL_WAIT_MS);
  }

  it('tail__should_poll_a_file_it_cannot_watch', async () => {
    // Given
    const dir = outputFolder('unwatchable');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(2));
    const watches = failWatches(dir);
    undo.push(watches);
    await selected(file);

    // When
    const before = (await outputs()).length;
    fs.appendFileSync(file, 'polled\n');
    const got = await shown(before, 'polled');

    // Then
    assert.ok(got, 'a file the system cannot watch did not show its new line within 3 s');
    assert.ok(watches.calls.some(([target]) => target === file), 'setup: the extension tried no watch of the file');
  });

  it('tail__should_poll_a_file_it_cannot_watch watch_fails', async () => {
    // Given
    const dir = outputFolder('watch-fails');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(2));
    const watchers = keepWatchers(dir);
    undo.push(watchers);
    await selected(file);
    const watcher = await waitFor(() => watchers.made[0], 3_000);
    assert.ok(watcher, 'setup: the extension made no watch of the file');
    let closed = false;
    watcher.on('close', () => {
      closed = true;
    });

    // When
    watcher.emit('error', Object.assign(new Error('EPERM: operation not permitted, watch'), { code: 'EPERM' }));
    const before = (await outputs()).length;
    fs.appendFileSync(file, 'after the error\n');
    const got = await shown(before, 'after the error');

    // Then
    assert.ok(got, 'after its watch failed, the file did not show its new line within 3 s');
    assert.ok(await waitFor(() => closed, 1_000), 'the watch that failed was not closed');
  });

  it('tail__should_read_a_change_made_during_a_read', async () => {
    // Given
    const dir = outputFolder('slow-tail');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(2));
    const reads = slowReads(dir);
    undo.push(reads);
    await selected(file);

    // When
    const before = (await outputs()).length;
    fs.appendFileSync(file, 'first change\n');
    await sleep(SLOW_CLOSE_MS / 3);
    fs.appendFileSync(file, 'during the read\n');
    const got = await shown(before, 'during the read');

    // Then
    assert.ok(got, 'a change made during a read did not show within 3 s');
    assert.equal(reads.most(), 1, 'two reads of one file were under way at once');
  });

  it('follow__should_poll_a_file_it_cannot_watch', async () => {
    // Given
    const dir = outputFolder('follow-unwatchable');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(3));
    const watches = failWatches(dir);
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    undo.push(watches, terminals);
    const terminal = await followed(file, terminals);
    assert.ok(await waitFor(() => terminal.text().includes('line 00003\r\n'), 3_000), 'the terminal did not show the file');

    // When
    fs.appendFileSync(file, 'polled\n');
    const got = await waitFor(() => terminal.text().includes('polled\r\n'), POLL_WAIT_MS);
    terminal.pty.close();

    // Then
    assert.ok(got, 'a followed file the system cannot watch did not show its new line within 3 s');
    assert.ok(watches.calls.some(([target]) => target === file), 'setup: Follow tried no watch of the file');
  });

  it('follow__should_read_one_gain_at_a_time', async () => {
    // Given
    const dir = outputFolder('follow-slow');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(3));
    const reads = slowReads(dir);
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    undo.push(reads, terminals);
    const terminal = await followed(file, terminals);
    assert.ok(await waitFor(() => terminal.text().includes('line 00003\r\n'), 3_000), 'the terminal did not show the file');

    // When
    fs.appendFileSync(file, 'first gain\n');
    await sleep(SLOW_CLOSE_MS / 3);
    fs.appendFileSync(file, 'second gain\n');
    await waitFor(() => terminal.text().includes('second gain\r\n'), POLL_WAIT_MS);
    await sleep(SLOW_CLOSE_MS * 2);
    const text = terminal.text();
    terminal.pty.close();

    // Then
    assert.equal(count(text, 'first gain\r\n'), 1, `the first gain did not show once: ${JSON.stringify(text.slice(-80))}`);
    assert.equal(count(text, 'second gain\r\n'), 1, `the second gain did not show once: ${JSON.stringify(text.slice(-80))}`);
    assert.ok(text.indexOf('first gain') < text.indexOf('second gain'), 'the gains showed out of order');
    assert.equal(reads.most(), 1, 'two reads of the file were under way at once');
  });

  it('follow__should_stop_at_a_file_replaced_by_a_symlink', async () => {
    // Given
    const dir = outputFolder('follow-replaced');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(3));
    const secret = path.join(path.dirname(dir), 'secret.txt');
    fs.writeFileSync(secret, `${lines(3, 'secret')}`);
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    undo.push(terminals);
    const terminal = await followed(file, terminals);
    assert.ok(await waitFor(() => terminal.text().includes('line 00003\r\n'), 3_000), 'the terminal did not show the file');
    const before = terminal.text().length;

    // When
    fs.rmSync(file);
    fs.symlinkSync(secret, file);
    fs.appendFileSync(secret, `${lines(20, 'more secret')}`);
    await sleep(QUIET_MS);
    const after = terminal.text().slice(before);
    terminal.pty.close();

    // Then
    assert.ok(!after.includes('secret'), `the terminal showed the file the symlink names: ${JSON.stringify(after.slice(0, 120))}`);
  });
});
