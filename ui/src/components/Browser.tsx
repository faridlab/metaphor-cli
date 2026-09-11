import { useMemo, useState } from 'react';
import { Box, Text, useApp, useInput } from 'ink';
import TextInput from 'ink-text-input';
import SelectInput from 'ink-select-input';
import { groupCommands, GROUP_ORDER } from '../grouping.ts';
import { filterCommands } from '../search.ts';
import type { FlatCommand } from '../types.ts';

export function Browser({ commands, onPick, onBack }: {
  commands: FlatCommand[];
  onPick: (cmd: FlatCommand) => void;
  onBack: () => void;
}): React.JSX.Element {
  const { exit } = useApp();
  const [query, setQuery] = useState('');
  const filtered = useMemo(() => filterCommands(commands, query), [commands, query]);
  const groups = useMemo(() => groupCommands(filtered), [filtered]);

  const items = useMemo(() => {
    const rows: Array<{ label: string; value: FlatCommand }> = [];
    for (const group of GROUP_ORDER) {
      for (const cmd of groups.get(group) ?? []) {
        rows.push({
          label: `${group.padEnd(10)} ${cmd.path.join(' ').padEnd(12)} ${cmd.node.about ?? ''}`,
          value: cmd,
        });
      }
    }
    return rows;
  }, [groups]);

  useInput((input, key) => {
    if (key.escape) onBack();
    if (key.ctrl && input === 'c') exit();
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>Commands</Text>
      <Text>search: </Text>
      <TextInput value={query} onChange={setQuery} placeholder="type to filter…" />
      {items.length === 0 ? (
        <Text dimColor>no commands match</Text>
      ) : (
        <SelectInput items={items} limit={14} onSelect={(item) => onPick(item.value)} />
      )}
      <Text dimColor>  ↑/↓ select · ↵ options · esc back · ctrl+c quit</Text>
    </Box>
  );
}
