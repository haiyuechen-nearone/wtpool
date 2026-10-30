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

## Agent marks

An agent line of the panel shows the mark of its model's maker and the mark of its harness. The
twelve rows of `src/shared/marks.ts` hold one mark each as a path, copied from the source
named below. Every mark is its owner's trademark; an icon set's licence covers the file and not the
brand. The licence texts of the MIT items follow the marks.

### Anthropic mark

- File: the row `anthropic` of `src/shared/marks.ts`, bundled in `out/html.js` and
  `media/webview.js`, which writes it into the page as an inline SVG.
- Source: `icons/anthropic.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names <https://www.anthropic.com>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Anthropic's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement. See [Anthropic Trademark
  Guidelines](https://www.anthropic.com/legal/trademark-guidelines).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### OpenAI mark

- File: the row `openai` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/openai.svg` of [Lobe Icons](https://github.com/lobehub/lobe-icons)
  (`@lobehub/icons-static-svg` 1.101.0, from npm).
- Licence: MIT, Copyright (c) 2023 LobeHub. The licence covers the icon collection, not the brand.
- Trademark: the mark is OpenAI's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement. See [OpenAI's brand
  guidelines](https://openai.com/brand/).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Google mark

- File: the row `google` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/google.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names <https://partnermarketinghub.withgoogle.com>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Google's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement. See [Google's brand
  guidelines](https://partnermarketinghub.withgoogle.com/brands/google/branding-guidelines/how-to-show-googles-brand/).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### DeepSeek mark

- File: the row `deepseek` of `src/shared/marks.ts`, bundled in `out/html.js` and
  `media/webview.js`, which writes it into the page as an inline SVG.
- Source: `icons/deepseek.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names <https://www.deepseek.com>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is DeepSeek's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement.
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Zhipu (Z.ai) mark

- File: the row `zhipu` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/zdotai.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names <https://z-cdn.chatglm.cn/z-ai/static/logo.svg>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Zhipu AI (Z.ai)'s. It names the maker of a model or the harness an agent
  runs in, and implies no sponsorship or endorsement.
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Qwen mark

- File: the row `qwen` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/qwen.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons) 16.36.0,
  from npm. Simple Icons names <https://qwen.ai>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Alibaba Cloud (Qwen)'s. It names the maker of a model or the harness an
  agent runs in, and implies no sponsorship or endorsement.
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Moonshot (Kimi) mark

- File: the row `moonshot` of `src/shared/marks.ts`, bundled in `out/html.js` and
  `media/webview.js`, which writes it into the page as an inline SVG.
- Source: `icons/kimi.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons) 16.36.0,
  from npm. Simple Icons names <https://moonshotai.github.io/Branding-Guide>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Moonshot AI (Kimi)'s. It names the maker of a model or the harness an agent
  runs in, and implies no sponsorship or endorsement. See [the Kimi branding
  guide](https://moonshotai.github.io/Branding-Guide).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Mistral mark

- File: the row `mistral` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/mistralai.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names <https://chat.mistral.ai>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Mistral AI's. It names the maker of a model or the harness an agent runs
  in, and implies no sponsorship or endorsement. See [Mistral's brand
  page](https://mistral.ai/brand).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### Meta mark

- File: the row `meta` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/meta.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons) 16.36.0,
  from npm. Simple Icons names <https://www.meta.com>
  as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), which
  Simple Icons applies to its package. CC0 waives no trademark right.
- Trademark: the mark is Meta's. It names the maker of a model or the harness an agent runs in, and
  implies no sponsorship or endorsement. See [Meta's brand
  resources](https://www.meta.com/brand/resources/meta/company-brand/).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### xAI mark

- File: the row `xai` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/xai.svg` of [Lobe Icons](https://github.com/lobehub/lobe-icons)
  (`@lobehub/icons-static-svg` 1.101.0, from npm).
- Licence: MIT, Copyright (c) 2023 LobeHub. The licence covers the icon collection, not the brand.
- Trademark: the mark is xAI's. It names the maker of a model or the harness an agent runs in, and
  implies no sponsorship or endorsement. See [xAI's brand
  guidelines](https://x.ai/legal/brand-guidelines).
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### MiniMax mark

- File: the row `minimax` of `src/shared/marks.ts`, bundled in `out/html.js` and `media/webview.js`,
  which writes it into the page as an inline SVG.
- Source: `icons/minimax.svg` of [Simple Icons](https://github.com/simple-icons/simple-icons)
  16.36.0, from npm. Simple Icons names
  <https://github.com/MiniMax-AI/MiniMax-01/blob/57cf223b177e99636c7711a0f179e9fdc9c38e8a/figures/minimax.svg> as the icon's source.
- Licence: the icon data is [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/) as in
  Simple Icons, which lists this icon's licence as "custom": [MiniMax's
  comment](https://github.com/simple-icons/simple-icons/pull/13982#issuecomment-3531627803) confirms
  that the file follows MiniMax's design guidelines and waives none of its rights.
- Trademark: the mark is MiniMax's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement.
- Modified: the path is copied unchanged, the view box is cut to the icon's ink, and the path is
  drawn in the colour of the line's text.

### OpenCode mark

- File: the row `opencode` of `src/shared/marks.ts`, bundled in `out/html.js` and
  `media/webview.js`, which writes it into the page as an inline SVG.
- Source: the frame and the inner block of `packages/identity/mark.svg` at commit
  1251a870cb384543c150c4a72fb101b55eec971b of
  [anomalyco/opencode](https://github.com/anomalyco/opencode), the repository of OpenCode (formerly
  sst/opencode). Simple Icons gives the same file as the source of its `opencode` icon.
- Licence: MIT, Copyright (c) 2025 opencode, the licence of the repository.
- Trademark: the mark is OpenCode's. It names the maker of a model or the harness an agent runs in,
  and implies no sponsorship or endorsement.
- Modified: the file's dark background square is left out, the frame is drawn in the colour of the
  line's text, its inner block (the file's grey square) in the same colour at 40% opacity, and the
  view box is cut to the frame's ink.

### Lobe Icons licence text

```text
MIT License

Copyright (c) 2023 LobeHub

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

### OpenCode licence text

```text
MIT License

Copyright (c) 2025 opencode

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
