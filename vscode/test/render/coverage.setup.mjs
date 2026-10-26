// Writes the V8 coverage of each Vitest worker before Vitest stops it, as a stopped worker writes
// none.
import v8 from 'node:v8';
import { afterAll } from 'vitest';

afterAll(() => {
  if (process.env.NODE_V8_COVERAGE) v8.takeCoverage();
});
