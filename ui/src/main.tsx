import { render } from 'ink';
import { App } from './app.tsx';

// The UI is keyboard-driven; without a TTY (CI, pipes) there is nothing
// useful to render, so exit with an actionable message instead of crashing
// inside a useInput hook.
if (process.stdin.isTTY !== true) {
  console.error('metaphor ui needs an interactive terminal (no TTY detected).');
  process.exit(1);
}

const launcher = process.env.METAPHOR_LAUNCHER ?? 'metaphor';
render(<App launcher={launcher} />, { exitOnCtrlC: false });
