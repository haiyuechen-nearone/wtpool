// The test runner VS Code loads into the extension host. A pass with a `currentRetry` above 0 in
// the JSON report shows as flaky in the harness's report.
//
// WTPOOL_VERIFY_HOST_ORDER=reverse runs the files, suites and checks in reverse order, so that a
// run shows a check that depends on an earlier one.
const fs = require('node:fs');
const path = require('node:path');
const Mocha = require('mocha');

function retries() {
  const text = process.env.WTPOOL_VERIFY_HOST_RETRIES ?? '2';
  if (!/^[0-2]$/.test(text)) throw new Error(`setup: WTPOOL_VERIFY_HOST_RETRIES must be 0, 1 or 2, not ${text}`);
  return Number(text);
}

function reverse(suite) {
  suite.suites.reverse();
  suite.tests.reverse();
  for (const child of suite.suites) reverse(child);
}

exports.run = async function run() {
  const mocha = new Mocha({
    ui: 'bdd',
    timeout: 180_000,
    retries: retries(),
    reporter: 'json',
    reporterOptions: { output: process.env.WTPOOL_VERIFY_RESULTS },
  });
  if (process.env.WTPOOL_VERIFY_HOST_GREP) {
    mocha.grep(new RegExp(process.env.WTPOOL_VERIFY_HOST_GREP));
    if (process.env.WTPOOL_VERIFY_HOST_INVERT === '1') mocha.invert();
  }
  for (const file of fs.readdirSync(__dirname).sort()) {
    if (file.endsWith('.test.cjs')) mocha.addFile(path.join(__dirname, file));
  }
  mocha.loadFiles();
  // run() must not load the files a second time.
  mocha.lazyLoadFiles(true);
  if (process.env.WTPOOL_VERIFY_HOST_ORDER === 'reverse') reverse(mocha.suite);
  await new Promise((resolve) => {
    mocha.run(() => resolve());
  });
};
