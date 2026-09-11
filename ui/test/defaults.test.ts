import { describe, expect, it } from 'vitest';
import { buildArgv } from '../src/argv.ts';
import { defaultAnswers, hasRequiredInputs, needsUserInput } from '../src/defaults.ts';
import type { FlatCommand } from '../src/types.ts';
import { AGENT_CMD, FIXTURE_MANIFEST, INFO_CMD, makeFlag } from './fixtures.ts';

describe('defaultAnswers', () => {
  it('turns toggles off and leaves valued flags unset', () => {
    expect(defaultAnswers(AGENT_CMD.node)).toEqual({ all: false });
  });

  it('a default run emits nothing beyond the subcommand path', () => {
    expect(buildArgv(INFO_CMD.node, INFO_CMD.path, defaultAnswers(INFO_CMD.node))).toEqual(['info']);
    expect(buildArgv(AGENT_CMD.node, AGENT_CMD.path, defaultAnswers(AGENT_CMD.node))).toEqual(['agent']);
  });
});

describe('hasRequiredInputs', () => {
  it('is true for a required flag without a default', () => {
    const node = { ...INFO_CMD.node, flags: [makeFlag({ id: 'name', long: 'name', required: true })] };
    expect(hasRequiredInputs(node)).toBe(true);
  });

  it('is false when the required flag carries a default', () => {
    const node = { ...INFO_CMD.node, flags: [makeFlag({ id: 'name', long: 'name', required: true, default_values: ['x'] })] };
    expect(hasRequiredInputs(node)).toBe(false);
  });

  it('is false for the fixture info command', () => {
    expect(hasRequiredInputs(INFO_CMD.node)).toBe(false);
  });
});

describe('needsUserInput', () => {
  const cmd = (path: string[], flags: any[] = []): FlatCommand => ({
    node: { ...FIXTURE_MANIFEST.subcommands[0]!, flags },
    path,
  });

  it('routes read-only diagnostics straight to run', () => {
    expect(needsUserInput(cmd(['doctor']))).toBe(false);
    expect(needsUserInput(INFO_CMD)).toBe(false);
  });

  it('routes creation commands to the input sheet even with no flags', () => {
    expect(needsUserInput(cmd(['add']))).toBe(true);
    expect(needsUserInput(cmd(['make']))).toBe(true);
    expect(needsUserInput(cmd(['module']))).toBe(true);
  });

  it('routes commands with positional arguments to the input sheet', () => {
    const positional = makeFlag({ id: 'name', long: null, short: null, positional: true, takes_value: true, value_name: 'NAME' });
    expect(needsUserInput(cmd(['show', 'project'], [positional]))).toBe(true);
  });

  it('ignores hidden flags when deciding', () => {
    const hiddenRequired = makeFlag({ id: 'secret', long: 'secret', required: true, hidden: true });
    expect(needsUserInput(cmd(['weird'], [hiddenRequired]))).toBe(false);
  });
});
