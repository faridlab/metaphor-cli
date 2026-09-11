import { PASSTHROUGH_KEY, type Answers, type CommandNode } from './types.ts';

/**
 * Minimal POSIX-ish word splitter for the passthrough "extra arguments"
 * field: splits on whitespace, honors double/single quotes and backslash
 * escapes. Avoids a dependency for one small job.
 */
export function splitArgs(raw: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quote: '"' | "'" | null = null;
  let started = false;
  for (let i = 0; i < raw.length; i++) {
    const ch = raw[i];
    if (quote != null) {
      if (ch === '\\' && quote === '"' && i + 1 < raw.length) {
        cur += raw[i + 1];
        i++;
        continue;
      }
      if (ch === quote) {
        quote = null;
        continue;
      }
      cur += ch;
      continue;
    }
    if (ch === '"' || ch === "'") {
      quote = ch;
      started = true;
      continue;
    }
    if (ch === '\\') {
      if (i + 1 < raw.length) {
        cur += raw[i + 1];
        i++;
      }
      started = true;
      continue;
    }
    if (ch === ' ' || ch === '\t' || ch === '\n') {
      if (started) {
        out.push(cur);
        cur = '';
        started = false;
      }
      continue;
    }
    cur += ch;
    started = true;
  }
  if (started || quote != null) out.push(cur);
  return out;
}

/**
 * Assemble the argv for a command from guided-form answers. Pure function:
 * manifest + answers → string[]. Deterministic order: subcommand path,
 * positionals (manifest order), valued flags, boolean flags, then raw
 * passthrough args.
 */
export function buildArgv(node: CommandNode, path: string[], answers: Answers): string[] {
  const argv = [...path];
  const valued: string[] = [];
  const booleans: string[] = [];

  for (const f of node.flags) {
    if (f.positional) {
      const v = answers[f.id];
      if (typeof v === 'string' && v !== '') argv.push(v);
      continue;
    }
    if (!f.takes_value) {
      if (answers[f.id] === true) {
        booleans.push(f.long ? `--${f.long}` : `-${f.short}`);
      }
      continue;
    }
    const v = answers[f.id];
    if (typeof v === 'string' && v !== '') {
      valued.push(f.long ? `--${f.long}=${v}` : `-${f.short} ${v}`);
    }
  }

  argv.push(...valued, ...booleans);

  if (node.trailing_var_arg && typeof answers[PASSTHROUGH_KEY] === 'string') {
    const raw = answers[PASSTHROUGH_KEY] as string;
    if (raw.trim() !== '') {
      argv.push(...splitArgs(raw));
    }
  }

  return argv;
}
