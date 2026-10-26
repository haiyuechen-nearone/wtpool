// Lint rules for the extension's sources, one group per check: lint__no_html_sinks,
// lint__no_colour_literals and lint__no_network_modules. vscode/eslint.config.mjs applies every
// group, and vscode/test/lint/check.mjs runs one group alone.

export const NETWORK_MODULES = ['net', 'http', 'https', 'tls', 'dgram', 'http2'];
const NETWORK_MODULE = `^(node:)?(${NETWORK_MODULES.join('|')})(/.*)?$`;
/** The same, without a slash: esquery ends a regular expression at the first one. */
const NETWORK_MODULE_QUERY = `^(node:)?(${NETWORK_MODULES.join('|')})($|\\x2F)`;

const SINK = 'X10: strings reach the DOM only through textContent or setAttribute';
const LINK = 'X10: no links, frames, scripts, forms or images from the webview code';
const COLOUR = 'X13: colours come only from --vscode-* variables and the theme body classes';
const NETWORK = 'X17: no networking module, no server';

const HEX = '#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})';
const COLOUR_FUNCTION = '\\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color|color-mix|light-dark)\\(';
/**
 * A string that is a hex colour, or that holds a CSS declaration with one, or a colour function.
 */
const COLOUR_TEXT = `/^\\s*${HEX}\\s*$|:\\s*${HEX}\\b|${COLOUR_FUNCTION}/`;

export const groups = {
  'html-sinks': {
    'no-eval': [],
    'no-implied-eval': [],
    'no-new-func': [],
    'no-script-url': [],
    'no-restricted-properties': [
      { object: 'document', property: 'write', message: SINK },
      { object: 'document', property: 'writeln', message: SINK },
      ...[
        'innerHTML',
        'outerHTML',
        'insertAdjacentHTML',
        'insertAdjacentText',
        'innerText',
        'outerText',
        'srcdoc',
        'createContextualFragment',
        'setHTMLUnsafe',
        'parseHTMLUnsafe',
      ].map((property) => ({ property, message: SINK })),
    ],
    'no-restricted-syntax': [
      {
        selector:
          'MemberExpression[computed=true][property.value=/^(innerHTML|outerHTML|insertAdjacentHTML|insertAdjacentText|innerText|outerText|srcdoc)$/]',
        message: SINK,
      },
      { selector: "NewExpression[callee.name='DOMParser']", message: SINK },
      {
        selector:
          "CallExpression[callee.property.name='setAttribute'][arguments.0.value=/^(on.*|href|src|srcset|srcdoc|style|action|formaction|xlink:href|ping|background|poster|data)$/i]",
        message: LINK,
      },
      {
        selector:
          "CallExpression[callee.property.name='setAttribute'][arguments.0.type!='Literal']",
        message: `${LINK}: setAttribute takes a literal attribute name`,
      },
      { selector: "CallExpression[callee.property.name=/^(setAttributeNS|setAttributeNode|setAttributeNodeNS)$/]", message: SINK },
      {
        selector:
          "CallExpression[callee.property.name='createElement'][arguments.0.value=/^(a|area|iframe|frame|frameset|script|object|embed|form|img|image|picture|source|video|audio|link|base|meta|style|svg|math|template|portal)$/i]",
        message: LINK,
      },
      {
        selector: "CallExpression[callee.property.name='createElement'][arguments.0.type!='Literal']",
        message: `${LINK}: createElement takes a literal tag name`,
      },
      { selector: "CallExpression[callee.property.name='createElementNS']", message: LINK },
    ],
  },
  colour: {
    'no-restricted-syntax': [
      { selector: `Literal[value=${COLOUR_TEXT}]`, message: COLOUR },
      { selector: `TemplateElement[value.raw=${COLOUR_TEXT}]`, message: COLOUR },
    ],
  },
  network: {
    'no-restricted-imports': [
      {
        paths: NETWORK_MODULES.flatMap((name) => [name, `node:${name}`]).map((name) => ({ name, message: NETWORK })),
        patterns: [{ regex: NETWORK_MODULE, message: NETWORK }],
      },
    ],
    'no-restricted-globals': [
      ...['fetch', 'WebSocket', 'XMLHttpRequest', 'EventSource', 'WebTransport', 'RTCPeerConnection'].map((name) => ({
        name,
        message: NETWORK,
      })),
    ],
    'no-restricted-properties': [
      ...['window', 'globalThis', 'self'].flatMap((object) =>
        ['fetch', 'WebSocket', 'XMLHttpRequest', 'EventSource'].map((property) => ({ object, property, message: NETWORK })),
      ),
      { object: 'navigator', property: 'sendBeacon', message: NETWORK },
    ],
    'no-restricted-syntax': [
      {
        selector: `CallExpression[callee.name='require'][arguments.0.value=/${NETWORK_MODULE_QUERY}/]`,
        message: NETWORK,
      },
      { selector: `ImportExpression[source.value=/${NETWORK_MODULE_QUERY}/]`, message: NETWORK },
      {
        selector: `TSImportEqualsDeclaration[moduleReference.expression.value=/${NETWORK_MODULE_QUERY}/]`,
        message: NETWORK,
      },
    ],
  },
};

/**
 * The ESLint rules of the named groups, each rule's options merged into one entry: a rule set
 * twice in a flat config keeps only its last options. Each value above lists a rule's options.
 */
export function rulesOf(names = Object.keys(groups)) {
  const options = {};
  for (const name of names) {
    for (const [rule, value] of Object.entries(groups[name])) {
      options[rule] = [...(options[rule] ?? []), ...value];
    }
  }
  const rules = {};
  for (const [rule, value] of Object.entries(options)) {
    rules[rule] =
      rule === 'no-restricted-imports'
        ? ['error', { paths: value.flatMap((v) => v.paths ?? []), patterns: value.flatMap((v) => v.patterns ?? []) }]
        : ['error', ...value];
  }
  return rules;
}

/** stylelint rules of lint__no_colour_literals. */
export const colourStylelintRules = {
  'color-no-hex': true,
  'color-named': 'never',
  'function-disallowed-list': [
    'rgb',
    'rgba',
    'hsl',
    'hsla',
    'hwb',
    'lab',
    'lch',
    'oklab',
    'oklch',
    'color',
    'color-mix',
    'light-dark',
    'device-cmyk',
    'contrast-color',
  ],
  'declaration-property-value-disallowed-list': {
    '/.*/': [
      '/\\b(?:Canvas|CanvasText|LinkText|VisitedText|ActiveText|ButtonFace|ButtonText|ButtonBorder|Field|FieldText|Highlight|HighlightText|SelectedItem|SelectedItemText|Mark|MarkText|GrayText|AccentColor|AccentColorText)\\b/',
    ],
  },
};

/** The source files the named groups lint, relative to vscode/. */
export const SOURCES = ['src/**/*.{ts,mts,cts,tsx,js,mjs,cjs,jsx}'];
export const STYLES = ['src/**/*.css', 'media/**/*.css'];
