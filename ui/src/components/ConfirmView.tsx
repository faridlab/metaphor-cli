import { Box, Text, useApp, useInput } from 'ink';
import { buildArgv } from '../argv.ts';
import { PASSTHROUGH_KEY, type Answers, type FlatCommand } from '../types.ts';

export function ConfirmView({ cmd, answers, onBack, onRun }: {
  cmd: FlatCommand;
  answers: Answers;
  onBack: () => void;
  onRun: (argv: string[]) => void;
}): React.JSX.Element {
  const { exit } = useApp();
  const argv = buildArgv(cmd.node, cmd.path, answers);
  const shown = argv.map((a) => (a.includes(' ') ? `"${a}"` : a)).join(' ');

  useInput((input, key) => {
    if (key.return) onRun(argv);
    if (key.escape) onBack();
    if (key.ctrl && input === 'c') exit();
  });

  const rows = Object.entries(answers).filter(([, v]) => v !== '' && v !== false);
  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>Confirm</Text>
      <Box
        borderStyle="round"
        flexDirection="column"
        paddingX={1}
      >
        <Text color="cyan">metaphor {shown}</Text>
      </Box>
      {rows.length > 0 && (
        <Box flexDirection="column">
          {rows.map(([k, v]) => (
            <Text key={k} dimColor>
              {k} = {typeof v === 'boolean' ? (v ? 'yes' : 'no') : v}
            </Text>
          ))}
        </Box>
      )}
      <Text dimColor>  ↵ run · esc back · ctrl+c quit</Text>
    </Box>
  );
}
