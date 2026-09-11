import { describe, expect, it } from 'vitest';
import { parseOverview } from '../src/overview.ts';
import { overviewLines } from '../src/overviewView.ts';

const RAW = JSON.stringify({
  version: 1,
  data: {
    workspace: { root: '/tmp/ws', project_count: 3 },
    current_project: { name: 'api', type: 'backend-service', path: './api', depends_on: [], depended_by: ['web'] },
    apps: [
      { name: 'api', type: 'backend-service', path: './api', exists: true, version: '0.1.0', git_branch: 'main', git_dirty: false },
      { name: 'web', type: 'webapp', path: './web', exists: true, version: null, git_branch: null, git_dirty: null },
    ],
    environments: [
      {
        name: 'dev',
        host: null,
        services: [{ service: 'api', tag: '0.0.2', deployed_at: '2026-09-10T02:00:00Z', deployed_by: 'bob' }],
      },
      {
        name: 'prod',
        host: 'prod.example.com',
        services: [{ service: 'api', tag: null, deployed_at: null, deployed_by: null }],
      },
    ],
    recent_deployments: [
      { env: 'dev', ts: '2026-09-10T03:00:00Z', action: 'push', status: 'failed', tag: '0.0.3', deployer: 'bob' },
    ],
    deploy_hint: null,
    plugins: [{ name: 'metaphor-agent', commands: ['agent'], installed: false }],
    health: { ok: 5, warn: 1, fail: 0, worst: ['[warn] env schema'] },
  },
});

describe('parseOverview', () => {
  it('parses the envelope', () => {
    const ov = parseOverview(RAW);
    expect(ov.workspace.root).toBe('/tmp/ws');
    expect(ov.environments).toHaveLength(2);
  });
});

describe('overviewLines', () => {
  it('renders apps, environments, versions, and health', () => {
    const lines = overviewLines(parseOverview(RAW));
    const joined = lines.join('\n');
    expect(joined).toContain('api');
    expect(joined).toContain('0.0.2 (by bob'); // deployed version per service
    expect(joined).toContain('prod.example.com');
    expect(joined).toContain('never deployed');
    expect(joined).toContain('health: 5 ok, 1 warn, 0 fail');
    expect(joined).toContain('metaphor-agent');
  });
});
