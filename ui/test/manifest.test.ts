import { describe, expect, it } from 'vitest';
import { parseManifest, flattenCommands } from '../src/manifest.ts';
import { FIXTURE_MANIFEST } from './fixtures.ts';

const ENVELOPE = JSON.stringify({ version: 1, data: FIXTURE_MANIFEST });

describe('parseManifest', () => {
  it('strips the banner before the JSON payload', () => {
    const raw = `⚡ Metaphor CLI\nOrchestrate independent project repos\n\n${ENVELOPE}`;
    const m = parseManifest(raw);
    expect(m.name).toBe('metaphor');
    expect(m.subcommands).toHaveLength(3);
  });

  it('rejects a wrong envelope version', () => {
    const raw = JSON.stringify({ version: 2, data: FIXTURE_MANIFEST });
    expect(() => parseManifest(raw)).toThrow(/version is not 1/);
  });

  it('rejects output with no JSON', () => {
    expect(() => parseManifest('nope')).toThrow(/no JSON payload/);
  });
});

describe('flattenCommands', () => {
  it('returns leaves plus flagged parents with their paths', () => {
    const flat = flattenCommands(FIXTURE_MANIFEST);
    const paths = flat.map((f) => f.path.join(' '));
    expect(paths).toContain('info');
    expect(paths).toContain('agent');
    expect(paths).toContain('show project');
    // `show` has neither flags nor own run behavior at top level, so it is
    // not a runnable entry itself.
    expect(paths).not.toContain('show');
  });
});
