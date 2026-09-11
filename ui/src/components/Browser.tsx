import { useMemo, useState } from 'react';
import { Box, Text, useApp, useInput } from 'ink';
import TextInput from 'ink-text-input';
import SelectInput from 'ink-select-input';
import { groupCommands, GROUP_ORDER } from '../grouping.ts';
import { filterCommands } from '../search.ts';
import type { FlatCommand } from '../types.ts';

/**
 * Command list. Enter runs the highlighted command with all defaults; o or
 * tab opens the options sheet for it instead. List rows carry explicit
 * string keys — ink-select-input falls back to stringifying `value`, which
 * for object values produces identical "[object Object]" keys for every row.
 */
export function Browser({ commands, onPick, onOptions, onBack }: {
  commands: FlatCommand[];
  onPick: (cmd: FlatCommand) => void;
  onOptions: (cmd: FlatCommand) => void;
  onBack: () => void;
}): React.JSX.Element {
  const { exit } = useApp();
  const [query, setQuery] = useState('');
  const [highlighted, setHighlighted] = useState<FlatCommand | null>(null);
  const filtered = useMemo(() => filterCommands(commands, query), [commands, query]);
  const groups = useMemo(() => groupCommands(filtered), [filtered]);

  const items = useMemo(() => {
    const rows: Array<{ key: string; label: string; value: FlatCommand }> = [];
    for (const group of GROUP_ORDER) {
      for (const cmd of groups.get(group) ?? []) {
        rows.push({
          key: cmd.path.join(' '),
          label: `${group.padEnd(10)} ${cmd.path.join(' ').padEnd(12)} ${cmd.node.about ?? ''}`,
          value: cmd,
        });
      }
    }
    return rows;
  }, [groups]);

  const current = highlighted ?? items[0]?.value ?? null;

  useInput((input, key) => {
    if (key.escape) onBack();
    if (key.ctrl && input === 'c') exit();
    if (current == null) return;
    // tab works while searching; the bare letter only when the search box is
    // empty, so it never swallows typed characters.
    if (key.tab) onOptions(current);
    if (input === 'o' && query === '') onOptions(current);
  });

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>Commands</Text>
      <Text>search: </Text>
      <TextInput value={query} onChange={setQuery} placeholder="type to filter…" />
      {items.length === 0 ? (
        <Text dimColor>no commands match</Text>
      ) : (
        <SelectInput
          items={items}
          limit={14}
          onHighlight={(item) => setHighlighted(item.value)}
          onSelect={(item) => onPick(item.value)}
        />
      )}
      <Text dimColor>  ↵ run · o/tab options · esc back · ctrl+c quit</Text>
    </Box>
  );
}
