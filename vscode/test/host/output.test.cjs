// R12, X19 and X15 in the extension host, with the fake wtpool (phase 3). The extension reads the
// tail of each output file that the last document lists for the selected row's actors, and of no
// other path; a listed path that does not end in `/tasks/<id>.output` gets a tail that says why,
// and neither a read nor a Follow. A growing file updates within a second. Escape sequences and C1 controls never reach
// the webview. A symlink or other non-regular file is refused. Watching stops on deselect and on a
// hidden tab. Follow opens a terminal whose Pseudoterminal streams the file with no control but SGR
// colours, and skips ahead of a writer that outruns it. The open strip's row gets every document.
// The webview's messages come in through the verify API's fromWebview. The files the extension
// opens, lstats and watches come from wrapped `fs.promises.open`, `fs.promises.lstat`, `fs.lstat`
// and `fs.watch`. The extension shares these with the checks, as it runs in this extension host.
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');
const vscode = require('vscode');

const { control, sleep, views, waitFor, wrap } = require('./support.cjs');
const {
  POOL,
  QUIET_MS,
  WT2,
  actor,
  lines,
  nextOutput,
  output,
  outputFolder,
  outputs,
  poolWith,
  start,
  watchFiles,
} = require('./outputs.cjs');

/**
 * A line of C1 control strings with their payloads: OSC 52 ended by BEL and by ST, an OSC 8 link,
 * DCS, APC, PM, SOS ended by `ESC \`, then a lone C1 and an 8-bit CSI SGR, both removed too.
 */
const C1_LINE = [
  'c1 \u009d52;c;aGk=\u0007bel \u009d52;c;aGk=\u009cst \u009d8;;https://example.com/\u009clink\u009d8;;\u009c ',
  '\u0090q#0;2;0;0;0\u009cdcs \u009fG a=T\u009capc \u009eprivate\u009cpm \u0098string\u001b\\sos ',
  '\u0085lone \u009b31mcsi8\u009b0m\n',
].join('');
/** {@link C1_LINE} as a tail and a Follow terminal show it. */
const C1_SHOWN = 'c1 bel st link dcs apc pm sos lone csi8';
/** The lines of a burst of about 5 MB, more than Follow reads in a tick. */
const BURST = 400_000;

describe('output tails (R12) and Follow (X19)', () => {
  let files;
  afterEach(async () => {
    files?.restore();
    files = undefined;
    await vscode.commands.executeCommand('workbench.action.closeAllEditors');
  });

  it('tail__should_read_only_the_selected_row', async () => {
    // Given
    const dir = outputFolder('selected');
    const mine = path.join(dir, 'b1.output');
    const other = path.join(dir, 'b2.output');
    fs.writeFileSync(mine, lines(3, 'wt2'));
    fs.writeFileSync(other, lines(3, 'wt3'));
    files = watchFiles();
    const api = await start(poolWith({ wt2: [actor('s2', [output(mine)])], wt3: [actor('s3', [output(other)])] }));
    await sleep(1000);
    const beforeSelect = files.seen.length;

    // When
    const count = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    const first = await nextOutput(count, WT2);
    await sleep(1500);

    // Then
    assert.equal(beforeSelect, 0, `the extension touched output files before a row was selected: ${JSON.stringify(files.seen)}`);
    assert.ok(first, 'selecting wt2 posted no output message for wt2');
    assert.deepEqual(first.tails.map((t) => t.path), [mine], 'the output message names other paths than wt2 lists');
    assert.deepEqual(first.tails[0].lines, ['wt2 00001', 'wt2 00002', 'wt2 00003']);
    assert.ok(!files.seen.some((s) => s.path === other), `the extension touched wt3's output while wt2 was selected: ${JSON.stringify(files.seen)}`);
    assert.ok(!(await outputs()).some((m) => m.tails.some((t) => t.path === other)), 'an output message carried wt3\'s output');
  });

  it('tail__should_update_a_growing_file_within_a_second', async () => {
    // Given: more than 64 KiB, so the read starts inside a line that must not show.
    const dir = outputFolder('growing');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, `partial ${'x'.repeat(70_000)}\n${lines(3)}`);
    const api = await start(poolWith({ wt2: [actor('s2', [output(file)])] }));
    let count = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    const first = await nextOutput(count, WT2);
    assert.ok(first, 'selecting wt2 posted no output message');

    // When
    const delays = [];
    for (const [index, text] of [
      'built \u001b[32mok\u001b[0m \u001b]0;title\u0007done\n',
      'progress 10%\rprogress 50%\rprogress 100%\n',
      Buffer.from([0x62, 0x61, 0x64, 0x20, 0xff, 0xfe, 0x0a]),
      C1_LINE,
    ].entries()) {
      count = (await outputs()).length;
      const wrote = Date.now();
      fs.appendFileSync(file, text);
      const want = ['built ok done', 'progress 100%', 'bad ��', C1_SHOWN][index];
      const got = await nextOutput(count, WT2, (m) => m.tails[0].lines.at(-1) === want, 3_000);
      delays.push(got ? Date.now() - wrote : Infinity);
      if (!got) {
        const last = (await outputs()).at(-1);
        assert.fail(`the tail did not show "${want}" within 3 s; last tail: ${JSON.stringify(last?.tails[0].lines.slice(-3))}`);
      }
    }
    // And: a tab shows as spaces to the next multiple of 8 columns, counted from the line's start
    // after the escapes are removed and the line is kept from its last carriage return.
    count = (await outputs()).length;
    fs.appendFileSync(file, 'cargo\tbuild\t--release\n\tindented\n12345678\tnine\nold\tline\r\u001b[32mok\u001b[0m\t1 passed\n');
    const tabbed = await nextOutput(count, WT2, (m) => m.tails[0].lines.at(-1) === 'ok      1 passed', 3_000);
    const burstStart = (await outputs()).length;
    const burstAt = Date.now();
    for (let n = 0; n < 40; n += 1) {
      fs.appendFileSync(file, `burst ${n}\n`);
      await sleep(25);
    }
    await sleep(1200);
    const burst = (await outputs()).slice(burstStart);
    const last = (await outputs()).at(-1);

    // Then
    assert.ok(delays.every((ms) => ms <= 1000), `a growing file updated in ${delays.join(', ')} ms, over 1 s`);
    assert.deepEqual(first.tails[0].lines, ['line 00001', 'line 00002', 'line 00003'], 'the cut first line of a read that starts inside the file shows, or a whole line is missing');
    assert.ok(tabbed, `the tabbed lines did not show; last tail: ${JSON.stringify((await outputs()).at(-1)?.tails[0].lines.slice(-4))}`);
    assert.deepEqual(
      tabbed.tails[0].lines.slice(-4),
      ['cargo   build   --release', '        indented', '12345678        nine', 'ok      1 passed'],
      'a tab does not show as spaces to the next multiple of 8 columns of the line as shown',
    );
    assert.ok(!JSON.stringify(burst).includes('\\u001b'), 'an escape sequence reached the webview');
    assert.ok(!/[\u0080-\u009f]/.test(JSON.stringify(await outputs())), 'a C1 control reached the webview');
    const seconds = (Date.now() - burstAt) / 1000;
    assert.ok(burst.length <= Math.ceil(seconds * 4) + 1, `${burst.length} output messages in ${seconds.toFixed(1)} s, over 4 a second`);
    assert.equal(last.tails[0].lines.at(-1), 'burst 39', 'the last line of the burst did not show');
    assert.equal(last.tails[0].lines.length, 15);
  });

  it('tail__should_refuse_symlinks_and_other_files', async () => {
    // Given
    const dir = outputFolder('refused');
    const secret = path.join(dir, 'secret.txt');
    fs.writeFileSync(secret, 'secret text\n');
    const link = path.join(dir, 'link.output');
    fs.symlinkSync(secret, link);
    const folderOutput = path.join(dir, 'folder.output');
    fs.mkdirSync(folderOutput);
    const fifo = path.join(dir, 'fifo.output');
    execFileSync('mkfifo', [fifo]);
    const gone = path.join(dir, 'gone.output');
    files = watchFiles();
    const api = await start(poolWith({ wt2: [actor('s2', [output(link), output(folderOutput, { task: 'b2' }), output(fifo, { task: 'b3' }), output(gone, { task: 'b4' })])] }));
    const count = (await outputs()).length;

    // When
    await api.fromWebview({ type: 'select', id: WT2 });
    const got = await nextOutput(count, WT2, (m) => m.tails.length === 4);
    await sleep(1500);

    // Then
    assert.ok(got, 'selecting wt2 posted no output message with its four outputs');
    for (const tail of got.tails) {
      assert.deepEqual(tail.lines, [], `${path.basename(tail.path)} shows lines`);
      assert.equal(typeof tail.error, 'string', `${path.basename(tail.path)} gives no reason it shows nothing`);
    }
    assert.ok(!JSON.stringify(await outputs()).includes('secret text'), 'the symlink\'s target reached the webview');
    assert.ok(!files.seen.some((s) => s.kind === 'open' && s.path === link), 'the extension opened the symlink');
    assert.ok(!files.seen.some((s) => s.path === secret), 'the extension touched the symlink\'s target');
  });

  it('tail__should_never_read_an_unlisted_path', async () => {
    // Given
    const dir = outputFolder('unlisted');
    const listed = path.join(dir, 'b1.output');
    const unlisted = path.join(dir, 'b9.output');
    fs.writeFileSync(listed, lines(2));
    fs.writeFileSync(unlisted, 'not listed\n');
    files = watchFiles();
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    try {
      const api = await start(poolWith({ wt2: [actor('s2', [output(listed)])] }));
      const count = (await outputs()).length;

      // When
      for (const message of [
        { type: 'select', id: unlisted },
        { type: 'select', id: `${POOL}/nowhere` },
        { type: 'follow', id: unlisted },
        { type: 'follow', id: '/etc/hosts' },
        { type: 'follow', id: listed, extra: true },
        { type: 'select', id: WT2, path: unlisted },
        { type: 'Select', id: WT2 },
        { type: 'follow', id: [listed] },
      ]) {
        await api.fromWebview(message);
      }
      await sleep(QUIET_MS);
      const quiet = (await outputs()).slice(count);
      await api.fromWebview({ type: 'select', id: WT2 });
      const got = await nextOutput(count, WT2);
      await sleep(1000);

      // Then
      assert.deepEqual(quiet, [], 'a message outside the set posted an output message');
      assert.equal(terminals.calls.length, 0, 'a follow of an unlisted path opened a terminal');
      assert.ok(got, 'a valid select posted no output message');
      assert.deepEqual(got.tails.map((t) => t.path), [listed]);
      assert.ok(!files.seen.some((s) => s.path === unlisted), `the extension touched a path no document lists: ${JSON.stringify(files.seen.filter((s) => s.path === unlisted))}`);
    } finally {
      terminals.restore();
    }
  });

  it('tail__should_never_read_an_unlisted_path task_output_shape', async () => {
    // Given: a listed task output, and listed paths of any other shape, each a file that exists.
    const tasks = outputFolder('shape');
    const root = path.dirname(tasks);
    const listed = path.join(tasks, 'b1.output');
    fs.writeFileSync(listed, lines(2));
    const others = [
      path.join(root, 'b2.output'),
      path.join(tasks, 'b3.log'),
      path.join(tasks, 'b 4.output'),
      path.join(tasks, `${'b'.repeat(65)}.output`),
      path.join(tasks, 'b5.output.old'),
      path.join(root, 'tasks-b6.output'),
    ];
    for (const other of others) fs.writeFileSync(other, 'secret text\n');
    files = watchFiles();
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    try {
      const listing = [output(listed), ...others.map((other, n) => output(other, { task: `b${n + 2}` }))];
      const api = await start(poolWith({ wt2: [actor('s2', listing)] }));
      const count = (await outputs()).length;

      // When
      await api.fromWebview({ type: 'select', id: WT2 });
      const got = await nextOutput(count, WT2, (m) => m.tails.length === listing.length && m.tails[0].lines.length > 0);
      for (const other of others) await api.fromWebview({ type: 'follow', id: other });
      await sleep(QUIET_MS);

      // Then
      assert.ok(got, 'selecting wt2 posted no output message with all its outputs');
      assert.deepEqual(got.tails.map((t) => t.path), listing.map((o) => o.path), 'the tails are not those wt2 lists, in its order');
      assert.deepEqual(got.tails[0].lines, ['line 00001', 'line 00002'], 'the task output file shows no tail');
      for (const tail of got.tails.slice(1)) {
        assert.deepEqual(tail.lines, [], `${tail.path} shows lines`);
        assert.equal(typeof tail.error, 'string', `${tail.path} gives no reason it shows nothing`);
      }
      assert.ok(!JSON.stringify(await outputs()).includes('secret text'), 'a path of another shape reached the webview');
      const touched = files.seen.filter((s) => others.includes(s.path));
      assert.deepEqual(touched, [], `the extension touched a path that is no task output: ${JSON.stringify(touched)}`);
      assert.equal(terminals.calls.length, 0, 'a follow of a path that is no task output opened a terminal');
    } finally {
      terminals.restore();
    }
  });

  it('tail__should_stop_on_deselect_and_hide', async () => {
    // Given
    const dir = outputFolder('stop');
    const file = path.join(dir, 'b1.output');
    fs.writeFileSync(file, lines(2));
    files = watchFiles();
    const api = await start(poolWith({ wt2: [actor('s2', [output(file)])] }));
    let count = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    assert.ok(await nextOutput(count, WT2), 'selecting wt2 posted no output message');

    // When: deselect, then the file grows.
    await api.fromWebview({ type: 'select', id: '' });
    await sleep(300);
    const touchedAt = files.seen.length;
    count = (await outputs()).length;
    fs.appendFileSync(file, 'after deselect\n');
    await sleep(QUIET_MS);
    const afterDeselect = (await outputs()).slice(count);
    const touchedAfterDeselect = files.seen.slice(touchedAt);

    // And: select again, hide the tab, and the file grows.
    count = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    const again = await nextOutput(count, WT2, (m) => m.tails[0].lines.at(-1) === 'after deselect');
    await vscode.commands.executeCommand('workbench.action.files.newUntitledFile');
    await sleep(500);
    const hiddenAt = files.seen.length;
    count = (await outputs()).length;
    fs.appendFileSync(file, 'while hidden\n');
    await sleep(QUIET_MS);
    const whileHidden = (await outputs()).slice(count);
    const touchedWhileHidden = files.seen.slice(hiddenAt);

    // Then
    assert.deepEqual(afterDeselect, [], 'the extension posted output after the row was deselected');
    assert.deepEqual(touchedAfterDeselect, [], `the extension kept reading after deselect: ${JSON.stringify(touchedAfterDeselect)}`);
    assert.ok(again, 'selecting wt2 again posted no tail with the new line');
    assert.deepEqual(whileHidden, [], 'the extension posted output while the tab was hidden');
    assert.deepEqual(touchedWhileHidden, [], `the extension kept reading while the tab was hidden: ${JSON.stringify(touchedWhileHidden)}`);
  });

  it('follow__should_stream_the_file_into_a_terminal', async () => {
    // Given
    const dir = outputFolder('follow');
    const file = path.join(dir, 'b1.output');
    // 50 lines, the last with a colour and a tab.
    fs.writeFileSync(file, `${lines(49)}colour \u001b[31mred\u001b[0m\tend\n`);
    const link = path.join(dir, 'b2.output');
    fs.symlinkSync(file, link);
    const terminals = wrap(vscode.window, 'createTerminal', () => false, () => ({ show() {}, dispose() {} }));
    files = watchFiles();
    try {
      const api = await start(poolWith({ wt2: [actor('s2', [output(file, { summary: 'cargo nextest' }), output(link, { task: 'b2' })], { label: 'build agent' })] }));

      // When
      await api.fromWebview({ type: 'follow', id: file });
      const options = await waitFor(() => terminals.calls[0]?.[0], 5_000);
      assert.ok(options, 'a follow opened no terminal');
      const pty = options.pty;
      assert.ok(pty && typeof pty.open === 'function' && typeof pty.onDidWrite === 'function', 'the terminal has no Pseudoterminal');
      const written = [];
      pty.onDidWrite((data) => written.push(data));
      pty.open(undefined);
      const shownAll = await waitFor(() => written.join('').includes('end\r\n'), 5_000);
      const startText = written.join('');
      const wrote = Date.now();
      // The 7-bit forms: SGR stays, OSC ended by BEL and by `ESC \`, DCS, CSI and a two character
      // escape go. Then C1 control strings, and every C0 control but tab, CR and LF.
      fs.appendFileSync(
        file,
        `new \u001b[31mred\u001b[0m \u001b]0;title\u0007line \u001b]52;c;aGk=\u001b\\st \u001bP1$r\u001b\\dcs \u001b[2Jcsi \u001b(Besc\n${C1_LINE}c0 \u0007a\u0008b\u000ec\u000fd\u0000e\u007ff\n`,
      );
      const grew = await waitFor(() => written.join('').includes('new '), 3_000);
      const delay = Date.now() - wrote;
      await waitFor(() => written.join('').includes('c0 abcdef\r\n'), 3_000);
      const grownText = written.join('').slice(startText.length);
      // A tab counts its columns across reads of the growing file too, from the last line start
      // or carriage return, with SGR colours taking none.
      fs.appendFileSync(file, 'abc');
      await waitFor(() => written.join('').endsWith('abc'), 3_000);
      const tabFrom = written.join('').length - 'abc'.length;
      fs.appendFileSync(file, '\tz\nprogress\t10%\rdone\tall\n\u001b[32mok\u001b[0m\tfine\n');
      await waitFor(() => written.join('').includes('fine\r\n'), 3_000);
      const tabText = written.join('').slice(tabFrom);
      // A rename makes the burst one gain, larger than a whole tick of reads.
      const burstFrom = written.join('').length;
      fs.writeFileSync(`${file}.next`, `${fs.readFileSync(file, 'utf8')}${lines(BURST, 'burst')}`);
      fs.renameSync(`${file}.next`, file);
      await waitFor(() => written.join('').includes(`burst ${BURST}\r\n`), 5_000);
      fs.appendFileSync(file, 'after the burst\n');
      await waitFor(() => written.join('').includes('after the burst\r\n'), 3_000);
      const burstText = written.join('').slice(burstFrom);
      fs.writeFileSync(file, 'started over\n');
      const restarted = await waitFor(() => written.join('').includes('started over'), 3_000);
      const restartText = written.join('');
      pty.close();
      await sleep(300);
      const closedAt = written.length;
      const touchedAt = files.seen.length;
      fs.appendFileSync(file, 'after close\n');
      await sleep(QUIET_MS);
      const writtenAfterClose = written.length;
      const touchedAfterClose = files.seen.slice(touchedAt);
      await api.fromWebview({ type: 'follow', id: link });
      const linkOptions = await waitFor(() => terminals.calls[1]?.[0], 5_000);
      const linkText = [];
      linkOptions?.pty.onDidWrite((data) => linkText.push(data));
      linkOptions?.pty.open(undefined);
      await waitFor(() => linkText.length, 3_000);
      linkOptions?.pty.close();

      // Then
      assert.equal(options.name, 'build agent · cargo nextest', 'the terminal is not named after the agent and task');
      assert.equal(options.shellPath, undefined, 'the terminal runs a shell');
      assert.ok(shownAll, 'the terminal did not show the file');
      assert.ok(!startText.includes('line 00010\r\n') && startText.includes('line 00011\r\n'), 'the terminal does not start with the last 40 lines');
      assert.ok(!/[^\r]\n/.test(startText), 'a line end reached the terminal without its carriage return');
      assert.ok(startText.endsWith('colour \u001b[31mred\u001b[0m      end\r\n'), `the last line does not show its tab as spaces to column 16: ${JSON.stringify(startText.slice(-40))}`);
      assert.ok(grew && delay <= 1000, `the new line took ${delay} ms to show`);
      assert.equal(
        grownText,
        `new \u001b[31mred\u001b[0m line st dcs csi esc\r\n${C1_SHOWN}\r\nc0 abcdef\r\n`,
        'the terminal got another escape sequence or control character than SGR colours, tab, CR and LF, or lost text',
      );
      assert.equal(
        tabText,
        'abc     z\r\nprogress        10%\rdone    all\r\n\u001b[32mok\u001b[0m      fine\r\n',
        'a tab of a growing file does not show as spaces to the next multiple of 8 columns',
      );
      const skipNote = /^\r\nwtpool: [^\r\n]*skipped[^\r\n]*\r\n/;
      assert.match(burstText, skipNote, `a burst no tick can take does not start with one line that says it skipped: ${JSON.stringify(burstText.slice(0, 200))}`);
      const lastBurst = Array.from({ length: 40 }, (_, n) => `burst ${BURST - 39 + n}\r\n`).join('');
      assert.equal(burstText.replace(skipNote, ''), `${lastBurst}after the burst\r\n`, 'the terminal did not skip to the last 40 lines of the burst, then follow on');
      assert.ok(restarted && /truncated/.test(restartText), 'a file that shrank did not start over with a note');
      assert.equal(writtenAfterClose, closedAt, 'the terminal wrote after it closed');
      assert.deepEqual(touchedAfterClose, [], 'the file was read after the terminal closed');
      assert.ok(linkOptions, 'a follow of a listed symlink opened no terminal to say why');
      assert.match(linkText.join(''), /symlink/, 'the terminal of a symlink does not say why it shows nothing');
      assert.ok(!linkText.join('').includes('line 00049'), 'the terminal followed a symlink');
    } finally {
      terminals.restore();
    }
  });

  it('detail__should_post_each_document_while_open', async () => {
    // Given
    const dir = outputFolder('detail');
    const first = path.join(dir, 'b1.output');
    const second = path.join(dir, 'b2.output');
    fs.writeFileSync(first, 'first task\n');
    fs.writeFileSync(second, 'second task\n');
    const docs = [
      poolWith({ wt2: [actor('s2', [output(first)])] }),
      poolWith({ wt2: [actor('s2', [output(first)], { state: 'waiting', reason: 'permission' })], wt2Facts: { git: { dirty: 3, ahead: null, behind: null, unreferenced: 0, operation: null, unknown: false } } }),
      poolWith({ wt2: [actor('s2', [output(second), output(first, { running: false, ended: '2026-10-20T12:00:30Z' })], { state: 'idle', action: { tool: 'Edit', target: 'src/lib.rs' } })] }),
    ];
    const api = await start(docs[0]);
    let count = (await outputs()).length;
    await api.fromWebview({ type: 'select', id: WT2 });
    assert.ok(await nextOutput(count, WT2), 'selecting wt2 posted no output message');

    // When
    const got = [];
    for (const doc of docs.slice(1)) {
      const viewCount = (await views()).length;
      count = (await outputs()).length;
      control(`emit ${JSON.stringify(doc)}`);
      const view = await waitFor(async () => (await views()).slice(viewCount).find((v) => v.status?.generated === doc.generated), 5_000);
      const paths = doc.pools[0].worktrees.find((w) => w.path === WT2).actors[0].outputs.map((o) => o.path);
      const changed = JSON.stringify(paths) !== JSON.stringify([first]);
      const tails = changed ? await nextOutput(count, WT2, (m) => JSON.stringify(m.tails.map((t) => t.path)) === JSON.stringify(paths), 3_000) : true;
      got.push({ view, tails });
    }

    // Then
    for (const [index, { view, tails }] of got.entries()) {
      assert.ok(view, `document ${index + 1} was not posted while wt2's strip was open`);
      assert.ok(tails, `document ${index + 1} lists other outputs for wt2, but no output message followed with them`);
    }
    assert.deepEqual(got[1].tails.tails.map((t) => t.lines), [['second task'], ['first task']]);
  });
});
