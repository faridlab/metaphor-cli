import type { Answers, CommandNode, FlatCommand } from './types.ts';

/**
 * Answers for "run with all defaults": boolean flags off, no valued flags
 * supplied. The CLI applies its own defaults for omitted flags, so a
 * default run emits nothing beyond the subcommand path itself.
 */
export function defaultAnswers(node: CommandNode): Answers {
  const answers: Answers = {};
  for (const f of node.flags) {
    if (f.hidden) continue;
    if (!f.positional && !f.takes_value) answers[f.id] = false;
  }
  return answers;
}

/**
 * True when running with defaults cannot succeed: some required input has
 * no default value, so the CLI's argument parser would reject the bare
 * invocation.
 */
export function hasRequiredInputs(node: CommandNode): boolean {
  return node.flags.some((f) => !f.hidden && f.required && f.default_values.length === 0);
}

/**
 * Top-level command names whose job is to create or register something new.
 * Even when every flag is optional, the user should fill in the target name
 * on the options sheet rather than run the bare command.
 */
const CREATES_ARTIFACTS = new Set([
  'init', 'add', 'new', 'create', 'make', 'module', 'apps', 'webapp',
  'seed', 'migration',
]);

/**
 * True when Enter on this command should open the input sheet instead of
 * running: it creates something (by name or by having required/positional
 * inputs), so the user is expected to fill fields in first.
 */
export function needsUserInput(cmd: FlatCommand): boolean {
  if (cmd.path.some((seg) => CREATES_ARTIFACTS.has(seg.toLowerCase()))) return true;
  return cmd.node.flags.some(
    (f) => !f.hidden && (
      (f.required && f.default_values.length === 0) || (f.positional && f.takes_value)
    ),
  );
}
