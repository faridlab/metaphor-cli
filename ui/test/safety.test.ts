import { describe, expect, it } from 'vitest';
import { isDestructive } from '../src/safety.ts';
import type { FlatCommand } from '../src/types.ts';
import { FIXTURE_MANIFEST } from './fixtures.ts';

const cmd = (path: string[], about: string | null): FlatCommand => ({
  node: { ...FIXTURE_MANIFEST.subcommands[0]!, about, flags: [], subcommands: [] },
  path,
});

describe('isDestructive', () => {
  it('treats diagnostics as safe', () => {
    expect(isDestructive(cmd(['doctor'], 'Run diagnostic checks against the workspace'))).toBe(false);
  });

  it('flags destructive subcommand segments', () => {
    expect(isDestructive(cmd(['ship', 'deploy', 'rollback'], 'Roll back a release'))).toBe(true);
    expect(isDestructive(cmd(['plugin', 'uninstall'], 'Remove a plugin'))).toBe(true);
    expect(isDestructive(cmd(['cache', 'clear'], 'Drop cached artifacts'))).toBe(true);
  });

  it('flags side-effectful commands by their description', () => {
    expect(isDestructive(cmd(['deploy'], 'Remote deployment (push, rollback, status, logs, migrate, exec)'))).toBe(true);
  });

  it('treats build, sync, and list as safe', () => {
    expect(isDestructive(cmd(['dev', 'build'], 'Build the workspace'))).toBe(false);
    expect(isDestructive(cmd(['sync'], 'Clone or update remote projects to their pinned ref'))).toBe(false);
    expect(isDestructive(cmd(['list'], 'Enumerate projects'))).toBe(false);
  });
});
