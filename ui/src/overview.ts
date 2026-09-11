import { spawnSync } from 'node:child_process';

/** Minimal view of the `metaphor overview --json` envelope. */
export interface OverviewData {
  workspace: { root: string; project_count: number };
  current_project: {
    name: string;
    type: string;
    path: string;
    depends_on: string[];
    depended_by: string[];
  } | null;
  apps: Array<{
    name: string;
    type: string;
    path: string;
    exists: boolean;
    version: string | null;
    git_branch: string | null;
    git_dirty: boolean | null;
  }>;
  environments: Array<{
    name: string;
    host: string | null;
    services: Array<{
      service: string;
      tag: string | null;
      deployed_at: string | null;
      deployed_by: string | null;
    }>;
  }> | null;
  recent_deployments: Array<{
    env: string;
    ts: string;
    action: string;
    status: string;
    tag: string;
    deployer: string;
  }>;
  deploy_hint: string | null;
  plugins: Array<{ name: string; commands: string[]; installed: boolean }>;
  health: { ok: number; warn: number; fail: number; worst: string[] };
}

export function parseOverview(raw: string): OverviewData {
  const start = raw.indexOf('{');
  if (start === -1) throw new Error('no JSON payload in overview output');
  const env = JSON.parse(raw.slice(start)) as { version?: unknown; data?: unknown };
  if (env.version !== 1 || env.data == null || typeof env.data !== 'object') {
    throw new Error('overview envelope version is not 1');
  }
  return env.data as unknown as OverviewData;
}

/** Fetch overview data; resolves to null when not inside a workspace. */
export function fetchOverview(launcher: string): OverviewData | null {
  const res = spawnSync(launcher, ['overview', '--json'], { encoding: 'utf8' });
  if (res.status !== 0) return null;
  try {
    return parseOverview(res.stdout ?? '');
  } catch {
    return null;
  }
}
