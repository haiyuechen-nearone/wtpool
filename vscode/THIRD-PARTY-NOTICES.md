# Third-party notices

The Worktrees extension is MIT licensed, Copyright (c) 2026 NEAR One Limited: see LICENSE. The
package also holds the third-party material below, each item under its own licence.

## Codicon font

- File: `media/codicon.ttf`, copied unmodified from `dist/codicon.ttf` of the npm package
  `@vscode/codicons` 0.0.46-24.
- Source: <https://github.com/microsoft/vscode-codicons>.
- Copyright (c) Microsoft Corporation.
- Licence: the font and icons are under the Creative Commons Attribution 4.0 International
  licence (CC BY 4.0), <https://creativecommons.org/licenses/by/4.0/legalcode>. The licensor offers
  them as-is and makes no warranties of any kind, as section 5 of that licence says. The
  package's code is MIT licensed, Copyright (c) Microsoft Corporation.

## Tab icon

- Files: `media/icon-light.svg` and `media/icon-dark.svg`, the Worktrees tab's icon.
- Source: the path of `src/icons/worktree.svg` in the npm package `@vscode/codicons` 0.0.46-24,
  <https://github.com/microsoft/vscode-codicons>.
- Copyright (c) Microsoft Corporation.
- Licence: CC BY 4.0, as the codicon font above. Modified: the path is filled `#424242` (light)
  and `#c5c5c5` (dark) instead of `currentColor`.

## Working icon

- The robot that marks a working row, an inline SVG in `out/html.js` from `src/html.ts`, was drawn
  for wtpool and holds no third-party material. It is MIT licensed with the rest of the extension.

## Marketplace icon

- The extension's icon, `media/marketplace-icon.png`, is rendered by `scripts/icon.mjs` from
  `media/marketplace-icon.svg`: the working robot above on a 128 px grid, its body a panel of three
  worktree rows. It was drawn for wtpool and holds no third-party material. It is MIT licensed with
  the rest of the extension.

## ajv

- File: `out/extension.js` bundles `ajv/dist/runtime/equal.js` and
  `ajv/dist/runtime/ucs2length.js`, which the extension's generated schema validator requires.
  `ucs2length` follows `ucs2decode` of punycode.js (<https://github.com/mathiasbynens/punycode.js>),
  as its source comment says.
- Source: the npm package `ajv` 8.20.0, <https://github.com/ajv-validator/ajv>.
- Licence: MIT.

```text
The MIT License (MIT)

Copyright (c) 2015-2021 Evgeny Poberezkin

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

## fast-deep-equal

- File: `out/extension.js` bundles `fast-deep-equal/index.js`, which ajv's `equal` requires.
- Source: the npm package `fast-deep-equal` 3.1.3, <https://github.com/epoberezkin/fast-deep-equal>.
- Licence: MIT.

```text
MIT License

Copyright (c) 2017 Evgeny Poberezkin

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
