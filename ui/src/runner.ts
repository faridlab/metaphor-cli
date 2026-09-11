import { spawn } from 'node:child_process';
import type { RunResult } from './types.ts';

export type { RunResult } from './types.ts';

export interface RunHandle {
  result: Promise<RunResult>;
  cancel(): void;
}

export function runCommand(opts: {
  launcher: string;
  args: string[];
  cwd: string;
  onStdout: (chunk: string) => void;
  onStderr: (chunk: string) => void;
}): RunHandle {
  const started = Date.now();
  const child = spawn(opts.launcher, opts.args, {
    cwd: opts.cwd,
    stdio: ['ignore', 'pipe', 'pipe'],
  });

  let cancelled = false;
  let killTimer: NodeJS.Timeout | null = null;
  let settle!: (r: RunResult) => void;
  const result = new Promise<RunResult>((resolve) => {
    settle = resolve;
  });

  child.stdout?.on('data', (d: Buffer) => opts.onStdout(d.toString()));
  child.stderr?.on('data', (d: Buffer) => opts.onStderr(d.toString()));
  child.on('error', (err) => {
    opts.onStderr(`${err.message}\n`);
    settle({ code: 127, signal: null, cancelled: false, durationMs: Date.now() - started });
  });
  child.on('close', (code, signal) => {
    if (killTimer) clearTimeout(killTimer);
    settle({ code, signal, cancelled, durationMs: Date.now() - started });
  });

  return {
    result,
    cancel() {
      cancelled = true;
      if (child.killed) return;
      child.kill('SIGINT');
      killTimer = setTimeout(() => child.kill('SIGKILL'), 3000);
    },
  };
}
