import type { FlatCommand, Manifest } from '../src/types.ts';

export function makeFlag(over: Partial<any> = {}): any {
  return {
    id: 'flag',
    long: 'flag',
    short: null,
    value_name: null,
    help: 'a flag',
    positional: false,
    takes_value: false,
    required: false,
    num_args: null,
    default_values: [],
    possible_values: [],
    global: false,
    hidden: false,
    ...over,
  };
}

export const FIXTURE_MANIFEST: Manifest = {
  name: 'metaphor',
  version: '0.2.2',
  about: 'Orchestrate independent project repos',
  flags: [makeFlag({ id: 'verbose', long: 'verbose', global: true })],
  subcommands: [
    {
      name: 'info',
      about: 'Summarize the workspace + which project cwd is inside',
      long_about: null,
      aliases: [],
      hidden: false,
      trailing_var_arg: false,
      allow_external_subcommands: false,
      flags: [
        makeFlag({ id: 'json', long: 'json', help: 'Emit JSON' }),
        // clap propagates global args into every subcommand during build().
        makeFlag({ id: 'verbose', long: 'verbose', global: true }),
      ],
      subcommands: [],
    },
    {
      name: 'agent',
      about: 'Install Claude Code skills and subagents',
      long_about: null,
      aliases: [],
      hidden: false,
      trailing_var_arg: true,
      allow_external_subcommands: true,
      flags: [
        makeFlag({ id: 'all', long: 'all', help: 'Run across every registered project' }),
        makeFlag({ id: 'parallel', long: 'parallel', takes_value: true, default_values: ['1'], help: 'Max concurrency' }),
        makeFlag({ id: 'project_type', long: 'project-type', takes_value: true, possible_values: [{ name: 'backend-service', help: null }], positional: false }),
      ],
      subcommands: [],
    },
    {
      name: 'show',
      about: 'Inspect registered projects',
      long_about: null,
      aliases: [],
      hidden: false,
      trailing_var_arg: false,
      allow_external_subcommands: false,
      flags: [],
      subcommands: [
        {
          name: 'project',
          about: 'Show one project',
          long_about: null,
          aliases: [],
          hidden: false,
          trailing_var_arg: false,
          allow_external_subcommands: false,
          flags: [makeFlag({ id: 'name', long: null, short: null, positional: true, takes_value: true, value_name: 'NAME' })],
          subcommands: [],
        },
      ],
    },
  ],
};

export const INFO_CMD: FlatCommand = {
  node: FIXTURE_MANIFEST.subcommands[0],
  path: ['info'],
};

export const AGENT_CMD: FlatCommand = {
  node: FIXTURE_MANIFEST.subcommands[1],
  path: ['agent'],
};

export const SHOW_PROJECT_CMD: FlatCommand = {
  node: FIXTURE_MANIFEST.subcommands[2].subcommands[0],
  path: ['show', 'project'],
};
