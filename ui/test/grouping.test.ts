import { describe, expect, it } from 'vitest';
import { groupCommands, EXCLUDED_COMMANDS } from '../src/grouping.ts';
import { filterCommands, fuzzyScore } from '../src/search.ts';
import { flattenCommands } from '../src/manifest.ts';
import { FIXTURE_MANIFEST } from './fixtures.ts';

const flat = flattenCommands(FIXTURE_MANIFEST);

describe('groupCommands', () => {
  it('maps known commands to their group', () => {
    const groups = groupCommands(flat);
    expect(groups.get('Agent')?.map((c) => c.path[0])).toContain('agent');
    expect(groups.get('Workspace')?.map((c) => c.path[0])).toContain('info');
  });

  it('excludes meta commands from the browser', () => {
    expect(EXCLUDED_COMMANDS.has('ui')).toBe(true);
    expect(EXCLUDED_COMMANDS.has('manifest')).toBe(true);
    expect(EXCLUDED_COMMANDS.has('repl')).toBe(true);
    const names = flat.map((c) => c.path.join(' '));
    for (const excluded of EXCLUDED_COMMANDS) {
      // no group may contain an excluded command
      for (const list of groupCommands(flat).values()) {
        for (const c of list) {
          expect(names).toContain(c.path.join(' '));
          expect(excluded).not.toBe(c.path.join(' '));
        }
      }
    }
  });
});

describe('search', () => {
  it('ranks exact prefix first', () => {
    const hits = filterCommands(flat, 'inf');
    expect(hits[0]?.path[0]).toBe('info');
  });

  it('returns -1 for non-matches', () => {
    expect(fuzzyScore('zzz', 'info — summarize')).toBe(-1);
  });
});
