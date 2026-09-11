import { useState } from 'react';
import { Box, Text, useApp, useInput } from 'ink';
import TextInput from 'ink-text-input';
import { buildArgv } from '../argv.ts';
import { defaultAnswers } from '../defaults.ts';
import { PASSTHROUGH_KEY, type Answers, type FlatCommand } from '../types.ts';

interface Field {
  id: string;
  label: string;
  help: string | null;
  kind: 'toggle' | 'text' | 'select';
  def: string | null;
  options: string[];
}

/**
 * Single-screen options sheet for one command. Every flag is a row the user
 * edits in place (no wizard steps), with a live preview of the assembled
 * command at the top. The last row runs the command; esc returns to the
 * command list without running anything.
 */
export function OptionsForm({ cmd, onBack, onSubmit }: {
  cmd: FlatCommand;
  onBack: () => void;
  onSubmit: (answers: Answers) => void;
}): React.JSX.Element {
  const { exit } = useApp();
  const fields: Field[] = buildFields(cmd.node);

  const [idx, setIdx] = useState(0);
  const [answers, setAnswers] = useState<Answers>(() => defaultAnswers(cmd.node));
  const [editing, setEditing] = useState<string | null>(null);

  const activate = (): void => {
    if (idx >= fields.length) {
      onSubmit(answers);
      return;
    }
    const field = fields[idx]!;
    if (field.kind === 'toggle') {
      setAnswers((a) => ({ ...a, [field.id]: !a[field.id] }));
    } else if (field.kind === 'select') {
      const current = typeof answers[field.id] === 'string' && answers[field.id] !== ''
        ? (answers[field.id] as string)
        : (field.def ?? field.options[0] ?? '');
      const at = field.options.indexOf(current);
      const next = field.options[(at + 1) % field.options.length] ?? field.options[0] ?? '';
      if (next !== '') setAnswers((a) => ({ ...a, [field.id]: next }));
    } else {
      setEditing(field.id);
    }
  };

  useInput((input, key) => {
    if (key.ctrl && input === 'c') {
      exit();
      return;
    }
    if (editing != null) {
      // The inline editor owns the keyboard; esc cancels the edit.
      if (key.escape) setEditing(null);
      return;
    }
    if (key.escape) {
      onBack();
      return;
    }
    if (key.upArrow || input === 'k') {
      setIdx((i) => Math.max(0, i - 1));
    } else if (key.downArrow || input === 'j') {
      setIdx((i) => Math.min(fields.length, i + 1));
    } else if (key.return || input === ' ') {
      activate();
    }
  });

  const argv = buildArgv(cmd.node, cmd.path, answers);
  const shown = argv.map((a) => (a.includes(' ') ? `"${a}"` : a)).join(' ');
  const editingField = editing != null ? fields.find((f) => f.id === editing) : undefined;

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text bold>Options — {cmd.path.join(' ')}</Text>
      <Box borderStyle="round" paddingX={1}>
        <Text color="cyan">metaphor {shown}</Text>
      </Box>
      {fields.map((field, i) => {
        const focused = i === idx;
        const value = answers[field.id];
        let display = '';
        let displayColor: 'green' | undefined = undefined;
        let dimValue = false;
        if (field.kind === 'toggle') {
          display = value === true ? 'yes' : 'no';
          displayColor = value === true ? 'green' : undefined;
        } else if (field.kind === 'select') {
          display = typeof value === 'string' && value !== '' ? value : (field.def ?? 'choose…');
          dimValue = !(typeof value === 'string' && value !== '');
        } else {
          display = typeof value === 'string' && value !== '' ? value : (field.def ?? '');
          dimValue = !(typeof value === 'string' && value !== '');
        }
        return (
          <Box key={field.id} gap={2}>
            <Text color={focused ? 'blue' : undefined}>{focused ? '❯' : ' '}</Text>
            <Text>{field.label.padEnd(18)}</Text>
            <Text color={displayColor} dimColor={dimValue}>{display === '' ? '—' : display}</Text>
            {focused && field.help != null && <Text dimColor>— {field.help}</Text>}
          </Box>
        );
      })}
      <Box gap={2}>
        <Text color={idx >= fields.length ? 'blue' : undefined}>{idx >= fields.length ? '❯' : ' '}</Text>
        <Text bold={idx >= fields.length} color={idx >= fields.length ? 'blue' : undefined}>
          Run command ↵
        </Text>
      </Box>
      {editingField != null && (
        <Box>
          <Text>{'  edit › '}</Text>
          <TextInput
            value={typeof answers[editingField.id] === 'string' ? (answers[editingField.id] as string) : ''}
            placeholder={editingField.def ?? ''}
            onChange={(v) => setAnswers((a) => ({ ...a, [editingField.id]: v }))}
            onSubmit={() => setEditing(null)}
          />
        </Box>
      )}
      <Text dimColor>  ↑/↓ move · space/↵ flip & edit · esc back · ctrl+c quit</Text>
    </Box>
  );
}

function buildFields(node: FlatCommand['node']): Field[] {
  const list: Field[] = [];
  const seen = new Set<string>();
  for (const f of node.flags) {
    if (f.hidden) continue;
    // clap propagates global flags into each subcommand; a manifest can carry
    // the same flag id twice, and duplicate rows would collide as React keys.
    if (seen.has(f.id)) continue;
    seen.add(f.id);
    const label = f.positional ? (f.value_name ?? f.id) : `--${f.long ?? f.short}`;
    if (f.possible_values.length > 0) {
      list.push({ id: f.id, label, help: f.help, kind: 'select', def: f.default_values[0] ?? null, options: f.possible_values.map((pv) => pv.name) });
    } else if (f.takes_value) {
      list.push({ id: f.id, label, help: f.help, kind: 'text', def: f.default_values[0] ?? null, options: [] });
    } else {
      list.push({ id: f.id, label, help: f.help, kind: 'toggle', def: null, options: [] });
    }
  }
  if (node.trailing_var_arg) {
    list.push({ id: PASSTHROUGH_KEY, label: 'extra arguments', help: 'Forwarded to the plugin verbatim', def: null, options: [], kind: 'text' });
  }
  return list;
}
