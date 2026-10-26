// Writes the V8 coverage of each Vitest worker before Vitest stops it, which a stopped worker
// would not write (verification.md, "Layers", cover of render).
import v8 from 'node:v8';
import { afterAll } from 'vitest';

afterAll(() => {
  if (process.env.NODE_V8_COVERAGE) v8.takeCoverage();
});
