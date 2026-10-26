// Finds VS Code once per run, downloading the pinned version when none is installed, and names it
// to the test processes.
import { exportVSCode } from '../support/vscode-app.mjs';

export default async function globalSetup() {
  const found = await exportVSCode();
  process.stdout.write(`e2e: VS Code from ${found.source}: ${found.app}\n`);
}
