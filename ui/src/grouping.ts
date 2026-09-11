import type { FlatCommand } from './types.ts';

/** Human-facing group per command name; unknown names land in "Other". */
export const COMMAND_GROUPS: Record<string, string> = {
  init: 'Workspace', list: 'Workspace', info: 'Workspace', show: 'Workspace',
  graph: 'Workspace', add: 'Workspace', sync: 'Workspace', doctor: 'Workspace',
  clean: 'Workspace', env: 'Workspace', cache: 'Workspace', compose: 'Workspace',
  overview: 'Workspace',
  schema: 'Scaffold', webapp: 'Scaffold', make: 'Scaffold', module: 'Scaffold',
  apps: 'Scaffold', proto: 'Scaffold', migration: 'Scaffold', seed: 'Scaffold',
  dev: 'Dev', lint: 'Dev', test: 'Dev', docs: 'Dev', config: 'Dev', jobs: 'Dev',
  build: 'Ship', docker: 'Ship', deploy: 'Ship', chaos: 'Ship',
  plugin: 'Plugins', agent: 'Agent',
};

export const GROUP_ORDER = ['Workspace', 'Scaffold', 'Dev', 'Ship', 'Plugins', 'Agent', 'Other'];

/** Meta or stdin-dependent commands the browser should never offer to run. */
export const EXCLUDED_COMMANDS = new Set(['ui', 'manifest', 'repl']);

export function groupCommands(commands: FlatCommand[]): Map<string, FlatCommand[]> {
  const groups = new Map<string, FlatCommand[]>();
  for (const cmd of commands) {
    if (cmd.path.length !== 1) continue; // nested leaves surface via their parent entry
    const name = cmd.path[0];
    if (EXCLUDED_COMMANDS.has(name)) continue;
    const group = COMMAND_GROUPS[name] ?? 'Other';
    const list = groups.get(group) ?? [];
    list.push(cmd);
    groups.set(group, list);
  }
  return groups;
}
