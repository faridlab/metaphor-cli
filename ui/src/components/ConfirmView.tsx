import { Box, Text, useApp, useInput } from 'ink';
import { buildArgv } from '../argv.ts';
import { isDestructive } from '../safety.ts';
import type { Answers, FlatCommand } from '../types.ts';

/**
 * Final gate before a command runs. Only destructive commands reach this
 * screen in the default flow; it shows the exact argv about to execute.
 */
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

  const dangerous = isDestructive(cmd);
  const rows = Object.entries(answers).filter(([, v]) => v !== '' && v !== false);
  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold color={dangerous ? 'yellow' : undefined}>
        {dangerous ? '⚠  This command changes things — confirm' : 'Confirm'}
      </Text>
      <Box
        borderStyle={dangerous ? 'round' : 'round'}
        borderColor={dangerous ? 'yellow' : undefined}
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
