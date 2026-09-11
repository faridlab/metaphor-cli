/**
 * Shared types for the metaphor-ui. Field names mirror the Rust manifest /
 * overview JSON verbatim (snake_case), so the manifest stays a faithful,
 * unmapped view of the CLI's own definitions.
 */

export interface PossibleValue {
  name: string;
  help: string | null;
}

export interface FlagNode {
  id: string;
  long: string | null;
  short: string | null;
  value_name: string | null;
  help: string | null;
  positional: boolean;
  takes_value: boolean;
  required: boolean;
  num_args: { min: number; max: number | null } | null;
  default_values: string[];
  possible_values: PossibleValue[];
  global: boolean;
  hidden: boolean;
}

export interface CommandNode {
  name: string;
  about: string | null;
  long_about: string | null;
  aliases: string[];
  hidden: boolean;
  trailing_var_arg: boolean;
  allow_external_subcommands: boolean;
  flags: FlagNode[];
  subcommands: CommandNode[];
}

export interface Manifest {
  name: string;
  version: string;
  about: string | null;
  flags: FlagNode[];
  subcommands: CommandNode[];
}

/** One runnable command flattened out of the manifest tree, with its subcommand path. */
export interface FlatCommand {
  node: CommandNode;
  path: string[];
}

/** Answers collected by the guided form, keyed by flag id. */
export type Answers = Record<string, string | boolean>;

/** Reserved answers key for trailing_var_arg commands ("extra arguments" field). */
export const PASSTHROUGH_KEY = '__passthrough__';

/** Result of a command run (mirrored from runner.ts). */
export interface RunResult {
  code: number | null;
  signal: string | null;
  cancelled: boolean;
  durationMs: number;
}
