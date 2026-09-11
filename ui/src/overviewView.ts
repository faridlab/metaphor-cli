import type { OverviewData } from './overview.ts';

/**
 * Render the overview payload as plain lines. Pure function so the Ink
 * component stays thin and the logic is unit-testable without a TTY.
 */
export function overviewLines(data: OverviewData): string[] {
  const lines: string[] = [];
  const ws = data.workspace;
  lines.push(`workspace: ${ws.root} (${ws.project_count} project(s))`);
  const cp = data.current_project;
  lines.push(
    cp ? `current: ${cp.name} (${cp.type})` : 'current: (not inside any registered project)',
  );

  if (data.apps.length > 0) {
    lines.push('');
    lines.push('apps:');
    for (const a of data.apps) {
      const ver = a.version ? `v${a.version}` : 'version?';
      const br = a.git_branch ? ` @${a.git_branch}${a.git_dirty ? '*' : ''}` : '';
      lines.push(`  ${a.name.padEnd(22)} ${a.type.padEnd(16)} ${ver}${br}`);
    }
  }

  if (data.environments) {
    lines.push('');
    lines.push('environments / currently deployed versions:');
    for (const env of data.environments) {
      const host = env.host ?? 'local';
      lines.push(`  ${env.name.padEnd(10)} ${host}`);
      for (const s of env.services) {
        const when = s.deployed_at ? `by ${s.deployed_by}, at ${s.deployed_at}` : 'never deployed';
        lines.push(`    ${s.service.padEnd(20)} ${s.tag ?? '-'} (${when})`);
      }
    }
  }

  if (data.recent_deployments.length > 0) {
    lines.push('');
    lines.push('recent deployments:');
    for (const r of data.recent_deployments.slice(0, 8)) {
      const ts = r.ts.replace('T', ' ').slice(0, 16);
      lines.push(`  ${ts}  ${r.env.padEnd(8)} ${r.status.padEnd(7)} ${r.tag} (by ${r.deployer})`);
    }
  }

  const missingPlugins = data.plugins.filter((p) => !p.installed).map((p) => p.name);
  if (missingPlugins.length > 0) {
    lines.push('');
    lines.push(`plugins missing: ${missingPlugins.join(', ')}`);
  }

  const h = data.health;
  lines.push('');
  lines.push(`health: ${h.ok} ok, ${h.warn} warn, ${h.fail} fail`);
  if (h.worst.length > 0) {
    for (const w of h.worst.slice(0, 4)) lines.push(`  ${w}`);
  }
  return lines;
}
