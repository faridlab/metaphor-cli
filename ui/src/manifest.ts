import { spawnSync } from 'node:child_process';
import type { CommandNode, FlatCommand, Manifest } from './types.ts';

export function parseManifest(raw: string): Manifest {
  const start = raw.indexOf('{');
  if (start === -1) throw new Error('no JSON payload in manifest output');
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw.slice(start));
  } catch (e) {
    throw new Error(`manifest JSON malformed: ${(e as Error).message}`);
  }
  const env = parsed as { version?: unknown; data?: unknown };
  if (env.version !== 1 || env.data == null || typeof env.data !== 'object') {
    throw new Error('manifest envelope version is not 1 — rebuild or upgrade metaphor');
  }
  return env.data as Manifest;
}

export function fetchManifest(launcher: string): Manifest {
  const res = spawnSync(launcher, ['manifest', '--json'], { encoding: 'utf8' });
  if (res.error) throw new Error(`could not run ${launcher}: ${res.error.message}`);
  if (res.status !== 0) {
    const err = (res.stderr ?? '').trim();
    throw new Error(`manifest failed (exit ${res.status})${err ? `: ${err}` : ''}`);
  }
  return parseManifest(res.stdout ?? '');
}

export function flattenCommands(m: Manifest): FlatCommand[] {
  const out: FlatCommand[] = [];
  function walk(node: CommandNode, path: string[]): void {
    if (path.length > 0 && (node.subcommands.length === 0 || node.flags.length > 0)) {
      out.push({ node, path });
    }
    for (const sub of node.subcommands) walk(sub, [...path, sub.name]);
  }
  const root: CommandNode = {
    name: m.name,
    about: m.about,
    long_about: null,
    aliases: [],
    hidden: false,
    trailing_var_arg: false,
    allow_external_subcommands: false,
    flags: m.flags,
    subcommands: m.subcommands,
  };
  walk(root, []);
  return out;
}
