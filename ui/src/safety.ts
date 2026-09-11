import type { FlatCommand } from './types.ts';

/**
 * Subcommand path segments that signal side effects the user should see
 * before they happen. Conservative: any command not matching never confirms.
 */
const DESTRUCTIVE_PATH = new Set([
  'rollback', 'migrate', 'migration', 'seed', 'uninstall', 'remove', 'rm',
  'delete', 'del', 'destroy', 'drop', 'reset', 'prune', 'clear', 'clean',
  'wipe', 'purge', 'revoke', 'stop', 'down', 'kill',
]);

/**
 * Description keywords that flag a side-effectful command whose path alone
 * looks harmless (e.g. the `deploy` umbrella command whose help text lists
 * rollback and migrate among its subactions).
 */
const DESTRUCTIVE_ABOUT =
  /\b(rollback|migrate|migration|seed|uninstall|delete|remove|destroy|drop|reset|prune|wipe|purge|overwrite|teardown)\b/i;

export function isDestructive(cmd: FlatCommand): boolean {
  if (cmd.path.some((seg) => DESTRUCTIVE_PATH.has(seg.toLowerCase()))) return true;
  return DESTRUCTIVE_ABOUT.test(cmd.node.about ?? '');
}
