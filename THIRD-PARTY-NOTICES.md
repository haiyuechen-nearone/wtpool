# Third-party notices

wtpool is MIT licensed, Copyright (c) 2026 NEAR One Limited: see [LICENSE](LICENSE). The items below
come from elsewhere and keep their own licences. Each copied file also names its source in a header
or field where its format allows one.

## In this repository

### Engineering standards

- File: [verify/standards/engineering-standards.md](verify/standards/engineering-standards.md).
- Source: [near/mpc](https://github.com/near/mpc), `docs/development/engineering-standards.md` at
  commit [bf7dbd2ad](https://github.com/near/mpc/blob/bf7dbd2addc3dd8ab8029f2422e799a30e3b4aa8/docs/development/engineering-standards.md)
  (2026-10-08).
- Licence: MIT, Copyright (c) 2025 NEAR One Limited.
- Modified: no. A note naming the source was added above the first heading.

### Google Rust Style Guide

- File: [verify/standards/google-rust-style-guide.md](verify/standards/google-rust-style-guide.md).
- Source: [google/styleguide](https://github.com/google/styleguide), `rustguide.md` at gh-pages
  commit [89e0e1b](https://github.com/google/styleguide/blob/89e0e1b454a1709f0957e4c184660dde87137209/rustguide.md)
  (2026-10-23), the source of <https://google.github.io/styleguide/rustguide.html>. Author: Google.
  Saved on 2026-10-26.
- Licence: the guide's text is [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/); its code
  samples are [Apache 2.0](https://www.apache.org/licenses/LICENSE-2.0), as the google/styleguide
  README says. The Apache 2.0 text is in [LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt).
- Modified: no. A note naming the source and licence was added above the first heading.

### VS Code theme colours

- Files: `verify/themes/dark.json`, `light.json`, `high-contrast.json`, `high-contrast-light.json`.
- Source: [microsoft/vscode](https://github.com/microsoft/vscode/tree/1.143.1) at tag 1.143.1:
  `extensions/theme-defaults/themes/` `dark_modern.json`, `light_modern.json`, `hc_black.json` and
  `hc_light.json` with their includes, the colour registry defaults of
  `src/vs/platform/theme/common/colors/`, and the font variables of
  `src/vs/workbench/contrib/webview/browser/themeing.ts`.
- Licence: MIT, Copyright (c) 2015 - present Microsoft Corporation.
- Modified: yes. The values are collected into one map of `--vscode-*` variables per theme, as
  VS Code 1.143.1 gives them to a webview, with hex colours in lower case. Each file's
  `description` field names the source.

### VS Code webview default styles

- File: `verify/themes/webview-default.css`.
- Source: microsoft/vscode at tag 1.143.1,
  [src/vs/workbench/contrib/webview/browser/pre/index.html](https://github.com/microsoft/vscode/blob/1.143.1/src/vs/workbench/contrib/webview/browser/pre/index.html)
  (`defaultStyles`).
- Licence: MIT, Copyright (c) 2015 - present Microsoft Corporation.
- Modified: the CSS was moved out of its script into a file and indented again. The header comment
  names the source.

### .gitignore

- File: [.gitignore](.gitignore).
- Source: GitHub's
  [Rust.gitignore](https://github.com/github/gitignore/blob/ccf8d89c54f13410f797cb08ac1f39f674d8106e/Rust.gitignore)
  template, commit ccf8d89.
- Licence: [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/).
- Modified: a header comment and the macOS `.DS_Store` entry at the end were added.

### Claude Code hook recordings

- Files: `verify/fixtures/claude-2.1.310/*.jsonl`, and the payloads of the live runs in
  `verify/reviews/live-contract-2026-10-25/fresh-scrub/` and
  `verify/reviews/live-permission-denied-2026-10-25/`.
- Source: the hook payloads that Claude Code, by Anthropic, sent during this repository's scripted
  contract sessions: version 2.1.310 for the fixtures, 2.1.306 for the two live runs.
  [The fixtures README](verify/fixtures/claude-2.1.310/README.md) says how they were recorded and
  scrubbed.
- Licence: none. They are run output, used under Anthropic's terms for Claude Code, and hold no
  Claude Code code. The payload format is Claude Code's.
- Modified: yes. Paths and ids were scrubbed (`cargo xtask scrub`).

## In the packaged extension

`npm run package` ships these in `vscode/wtpool.vsix`, with [vscode/LICENSE](vscode/LICENSE) and
[vscode/THIRD-PARTY-NOTICES.md](vscode/THIRD-PARTY-NOTICES.md), which repeats these notices with
the licence texts. The font and the bundled code are not in git: the build takes both from
`vscode/node_modules`. The tab icons are in git.

### Codicon font

- File: `media/codicon.ttf` in the package, which `vscode/scripts/build.mjs` copies from
  `@vscode/codicons/dist/codicon.ttf`.
- Source: [@vscode/codicons](https://github.com/microsoft/vscode-codicons) 0.0.46-24, from npm.
- Licence: the font and icons are [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/); the
  package's code is MIT. Copyright (c) Microsoft Corporation.
- Modified: no.

### Tab icon

- Files: [vscode/media/icon-light.svg](vscode/media/icon-light.svg) and
  [vscode/media/icon-dark.svg](vscode/media/icon-dark.svg), `media/` in the package: the Worktrees
  tab's `iconPath`.
- Source: the path of `src/icons/worktree.svg` in
  [@vscode/codicons](https://github.com/microsoft/vscode-codicons) 0.0.46-24, from npm.
- Licence: [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). Copyright (c) Microsoft
  Corporation.
- Modified: yes. The path is filled `#424242` (light) and `#c5c5c5` (dark) instead of
  `currentColor`, and a comment names the source and licence.

### ajv runtime and fast-deep-equal

- File: `out/extension.js` in the package. esbuild bundles `ajv/dist/runtime/equal` and
  `ajv/dist/runtime/ucs2length`, which the generated validator `vscode/src/generated/validate.js`
  (ajv standalone output, written by `vscode/scripts/generate.mjs`) requires, and
  `fast-deep-equal`, which `equal` requires.
- Source: [ajv](https://github.com/ajv-validator/ajv) 8.20.0 and
  [fast-deep-equal](https://github.com/epoberezkin/fast-deep-equal) 3.1.3, from npm. ajv's
  `ucs2length` follows `ucs2decode` of [punycode.js](https://github.com/mathiasbynens/punycode.js),
  as its source comment says.
- Licence: MIT. ajv: Copyright (c) 2015-2021 Evgeny Poberezkin. fast-deep-equal: Copyright (c)
  2017 Evgeny Poberezkin.
- Modified: no, only bundled.

## Dependencies outside the repository

The crates in `Cargo.lock` and `xtask/Cargo.lock` and the npm packages in
`vscode/package-lock.json` and `vscode/test/package-lock.json` are downloaded at build time and keep
their own licences. The `wtpool` binary links the crates of `Cargo.lock`, all under MIT, Apache-2.0,
ISC, Unlicense, Unicode-3.0 or CC0-1.0. A `wtpool` binary given to others should carry their notices.

Everything else in this repository is wtpool's own work under [LICENSE](LICENSE), including the hand
written hook fixtures in `verify/fixtures/hooks/`, the scenarios, the screenshots and the
hand-drawn SVGs.

## MIT licence text

This text applies to each MIT item above, with that item's copyright line:

```text
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
