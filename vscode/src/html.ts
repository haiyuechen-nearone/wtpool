// Imports only types from `vscode`, so plain Node can load this module.
import type { Webview } from 'vscode';

/**
 * The page's first script, inline at the top of <head>. VS Code hands the messages it queued for
 * a loading page over at the page's `load` event or after 200 ms, and a message with no listener
 * yet is lost, while the host posts the last view only once, on show. So this listens as the page
 * is parsed, keeps the latest message of each type, and hands them over in the order they came to
 * the consumer webview.js attaches through `window.wtpoolEarly`, then passes every message on.
 */
const EARLY = `(() => {
  let consumer = null;
  const latest = new Map();
  window.addEventListener('message', (event) => {
    if (consumer) return consumer(event.data);
    const type = event.data?.type;
    latest.delete(type);
    latest.set(type, event.data);
  });
  window.wtpoolEarly = {
    attach(next) {
      consumer = next;
      for (const data of latest.values()) next(data);
      latest.clear();
    },
  };
})();`;

/**
 * The working icon: a robot drawn for wtpool, MIT like the rest of the extension, from no
 * third-party source. It is static markup in a <template> that the rows copy, so no code creates
 * an element of a namespace that could link or load. One 16 px grid, about 1 px lines, in
 * `currentColor`. Each arm is its own path, so icons.css can swing it about its shoulder.
 */
const ROBOT = [
  '<svg class="robot" viewBox="0 0 16 16" width="16" height="16" aria-hidden="true" focusable="false">',
  '<path class="robot-frame" fill="currentColor" d="M8 .5a1 1 0 1 1 0 2 1 1 0 0 1 0-2ZM7.5 2h1v1.5h-1ZM7 8h2v1H7Z"/>',
  '<path class="robot-frame" fill="currentColor" fill-rule="evenodd" d="M4.5 3h7A1.5 1.5 0 0 1 13 4.5v2A1.5 1.5 0 0 1 11.5 8h-7A1.5 1.5 0 0 1 3 6.5v-2A1.5 1.5 0 0 1 4.5 3Zm0 1a.5.5 0 0 0-.5.5v2a.5.5 0 0 0 .5.5h7a.5.5 0 0 0 .5-.5v-2a.5.5 0 0 0-.5-.5ZM6.5 4.5a1 1 0 1 1 0 2 1 1 0 0 1 0-2Zm3 0a1 1 0 1 1 0 2 1 1 0 0 1 0-2ZM5 9h6a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1v-4a1 1 0 0 1 1-1Zm0 1v4h6v-4Z"/>',
  '<path class="robot-arm robot-arm-left" fill="currentColor" d="M3.5 9.75a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0v-3a.75.75 0 0 1 .75-.75Z"/>',
  '<path class="robot-arm robot-arm-right" fill="currentColor" d="M12.5 9.75a.75.75 0 0 1 .75.75v3a.75.75 0 0 1-1.5 0v-3a.75.75 0 0 1 .75-.75Z"/>',
  '</svg>',
].join('');

/** The page of the Worktrees tab. `webview.options.localResourceRoots[0]` must be vscode/media. */
export function buildHtml(webview: Webview, nonce: string): string {
  const [media] = webview.options.localResourceRoots ?? [];
  if (!media) throw new Error('the webview has no local resource root');
  const asset = (file: string) => webview.asWebviewUri(media.with({ path: `${media.path}/${file}` })).toString();
  const csp = [
    "default-src 'none'",
    `script-src 'nonce-${nonce}'`,
    `style-src ${webview.cspSource}`,
    `font-src ${webview.cspSource}`,
    "require-trusted-types-for 'script'",
  ].join('; ');
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1">
<script nonce="${nonce}">${EARLY}</script>
<link rel="stylesheet" href="${asset('webview.css')}">
<title>Worktrees</title>
</head>
<body>
<template id="robot">${ROBOT}</template>
<script nonce="${nonce}" src="${asset('webview.js')}"></script>
</body>
</html>
`;
}
