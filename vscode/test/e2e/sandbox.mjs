// The environment base of every sandbox process: a home, a temporary folder, the XDG folders and
// the time zone, so nothing of the owner's reaches VS Code, wtpool or claude.
import path from 'node:path';

/**
 * The variables every sandbox process gets: `HOME`, `TMPDIR` when `tmp` is given, `TZ` and the
 * four `XDG_*_HOME` folders, by default below `home`, and `WTPOOL_VERIFY_SANDBOX`. `xdg` replaces
 * single folders: {config, cache, data, state}. Callers extend the result.
 */
export function sandboxEnv({ home, tmp, tz, xdg = {} }) {
  const variables = {
    PATH: process.env.PATH,
    HOME: home,
    XDG_CONFIG_HOME: xdg.config ?? path.join(home, '.config'),
    XDG_CACHE_HOME: xdg.cache ?? path.join(home, '.cache'),
    XDG_DATA_HOME: xdg.data ?? path.join(home, '.local', 'share'),
    XDG_STATE_HOME: xdg.state ?? path.join(home, '.local', 'state'),
    TZ: tz,
    WTPOOL_VERIFY_SANDBOX: '1',
  };
  if (tmp) variables.TMPDIR = tmp;
  return variables;
}
