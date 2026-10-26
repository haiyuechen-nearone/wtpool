// The test runner VS Code loads into the extension host: Mocha with the JSON reporter writing
// to WTPOOL_VERIFY_RESULTS. A check may take 180 s and is retried twice; the reporter's
// `currentRetry` of a pass tells the harness it needed a retry, which the report shows as flaky.
//
// For order tests, WTPOOL_VERIFY_HOST_RETRIES sets the retries (0 runs each check once) and
// WTPOOL_VERIFY_HOST_ORDER=reverse runs the files, suites and checks in reverse order.
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
