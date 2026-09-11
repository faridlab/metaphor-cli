import { Box, Text, useApp, useInput } from 'ink';
import { overviewLines } from '../overviewView.ts';
import type { OverviewData } from '../overview.ts';

export function Overview({ data, onOpen }: {
  data: OverviewData | null;
  onOpen: () => void;
}): React.JSX.Element {
  const { exit } = useApp();
  useInput((input, key) => {
    if (input === 'q') exit();
    if (input === 'b' || key.return) onOpen();
  });

  if (data == null) {
    return (
      <Box flexDirection="column" paddingX={1}>
        <Text>Workspace overview unavailable (run inside a metaphor workspace).</Text>
        <Text dimColor>  ↵/b commands · ctrl+c quit</Text>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>⚡ Metaphor — Workspace overview</Text>
      {overviewLines(data).map((line, i) => (
        <Text key={i}>{line === '' ? ' ' : line}</Text>
      ))}
      <Text dimColor>  ↵/b commands · q quit</Text>
    </Box>
  );
}
