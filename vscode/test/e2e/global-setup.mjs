// Finds VS Code once per e2e run, before any check starts, downloading the pinned version when no
// install is found (support/vscode-app.mjs), and names it to the test processes.
import { exportVSCode } from '../support/vscode-app.mjs';

export default async function globalSetup() {
  const found = await exportVSCode();
  process.stdout.write(`e2e: VS Code from ${found.source}: ${found.app}\n`);
}
