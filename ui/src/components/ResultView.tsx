import { Box, Text, useApp, useInput } from 'ink';
import type { FlatCommand, RunResult } from '../types.ts';

export function ResultView({ cmd, argv, result, onBack, onRerun }: {
  cmd: FlatCommand;
  argv: string[];
  result: RunResult;
  onBack: () => void;
  onRerun: () => void;
}): React.JSX.Element {
  const { exit } = useApp();
  useInput((input, key) => {
    if (key.return || input === 'b') onBack();
    if (input === 'r') onRerun();
    if (key.ctrl && input === 'c') exit();
  });

  const banner = result.cancelled
    ? '· cancelled'
    : result.code === 0
      ? '✓ exited 0'
      : `✗ exited ${result.code ?? `signal ${result.signal ?? '?'}`}`;

  const color = result.cancelled ? 'yellow' : result.code === 0 ? 'green' : 'red';

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>{cmd.path.join(' ')}</Text>
      <Text dimColor>$ metaphor {argv.join(' ')}</Text>
      <Text color={color}>{banner} ({(result.durationMs / 1000).toFixed(1)}s)</Text>
      <Text dimColor>  ↵/b back · r re-run · ctrl+c quit</Text>
    </Box>
  );
}
