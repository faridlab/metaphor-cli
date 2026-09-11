import type { FlatCommand } from './types.ts';

/**
 * Case-insensitive subsequence fuzzy matcher. For ~35 commands any scorer
 * works; this avoids a dependency.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q.length === 0) return 0;
  let score = 0;
  let ti = 0;
  for (const ch of q) {
    const idx = t.indexOf(ch, ti);
    if (idx === -1) return -1;
    if (idx === ti) {
      score += 2;
    } else {
      score += 1;
    }
    if (idx === 0) score += 3;
    ti = idx + 1;
  }
  return score;
}

/** Score a command against the query across name, aliases, and about text. */
export function filterCommands(commands: FlatCommand[], query: string): FlatCommand[] {
  if (query.trim() === '') return commands;
  const scored: Array<{ cmd: FlatCommand; score: number }> = [];
  for (const cmd of commands) {
    const name = cmd.path.join(' ');
    const hay = [name, ...cmd.node.aliases, cmd.node.about ?? ''].join(' ');
    const score = fuzzyScore(query, hay);
    if (score >= 0) {
      scored.push({ cmd, score });
    }
  }
  scored.sort((a, b) => b.score - a.score);
  return scored.map((s) => s.cmd);
}
