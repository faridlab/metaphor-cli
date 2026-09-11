import { describe, expect, it } from 'vitest';
import { buildArgv, splitArgs } from '../src/argv.ts';
import { PASSTHROUGH_KEY } from '../src/types.ts';
import { AGENT_CMD, INFO_CMD, SHOW_PROJECT_CMD } from './fixtures.ts';

describe('buildArgv', () => {
  it('emits a boolean flag when true', () => {
    const argv = buildArgv(INFO_CMD.node, INFO_CMD.path, { json: true, verbose: false });
    expect(argv).toEqual(['info', '--json']);
  });

  it('omits a boolean flag when false', () => {
    const argv = buildArgv(INFO_CMD.node, INFO_CMD.path, { json: false });
    expect(argv).toEqual(['info']);
  });

  it('emits valued flags explicitly', () => {
    const argv = buildArgv(AGENT_CMD.node, AGENT_CMD.path, { parallel: '4', verbose: false });
    expect(argv).toEqual(['agent', '--parallel=4']);
  });

  it('appends positionals in manifest order', () => {
    const argv = buildArgv(SHOW_PROJECT_CMD.node, SHOW_PROJECT_CMD.path, { name: 'web', verbose: false });
    expect(argv).toEqual(['show', 'project', 'web']);
  });

  it('splits passthrough extra args with quote handling', () => {
    const argv = buildArgv(AGENT_CMD.node, AGENT_CMD.path, {
      [PASSTHROUGH_KEY]: 'install --global "my skill"',
      verbose: false,
    });
    expect(argv).toEqual(['agent', 'install', '--global', 'my skill']);
  });

  it('appends --verbose when the toggle is set', () => {
    const argv = buildArgv(INFO_CMD.node, INFO_CMD.path, { json: false, verbose: true });
    expect(argv).toEqual(['info', '--verbose']);
  });
});

describe('splitArgs', () => {
  it('splits on whitespace', () => {
    expect(splitArgs('a b  c')).toEqual(['a', 'b', 'c']);
  });

  it('keeps quoted content together', () => {
    expect(splitArgs('say "hello world"')).toEqual(['say', 'hello world']);
  });

  it('supports single quotes and escapes', () => {
    expect(splitArgs("'two words'")).toEqual(['two words']);
    expect(splitArgs('a\\ b')).toEqual(['a b']);
  });
});
