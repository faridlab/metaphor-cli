import { useState } from 'react';
import { Box, Text, useApp, useInput } from 'ink';
import TextInput from 'ink-text-input';
import SelectInput from 'ink-select-input';
import { PASSTHROUGH_KEY, type Answers, type FlatCommand } from '../types.ts';

interface Field {
  id: string;
  label: string;
  help: string | null;
  kind: 'text' | 'toggle' | 'select';
  def: string | null;
  options: string[];
}

export function OptionsForm({ cmd, onBack, onSubmit }: {
  cmd: FlatCommand;
  onBack: () => void;
  onSubmit: (answers: Answers) => void;
}): React.JSX.Element {
  const { exit } = useApp();
  const fields: Field[] = buildFields(cmd.node);

  const [idx, setIdx] = useState(0);
  const [answers, setAnswers] = useState<Answers>(() => initAnswers(fields));

  const field = fields[idx];

  useInput((input, key) => {
    if (key.escape) onSubmit(answers);
    if (key.ctrl && input === 'c') exit();
  });

  if (field == null) {
    return (
      <Box flexDirection="column" paddingX={1}>
        <Text>No options — press ↵ to confirm</Text>
        <TextInput value="" onChange={() => {}} onSubmit={() => onSubmit(answers)} />
      </Box>
    );
  }

  const advance = (): void => {
    if (idx + 1 < fields.length) setIdx(idx + 1);
    else onSubmit(answers);
  };

  const setAnswer = (v: string | boolean): void => {
    setAnswers((a) => ({ ...a, [field.id]: v }));
    advance();
  };
  // Render the current field by kind.
  if (field.kind === 'toggle' || field.kind === 'select') {
    const items = (field.kind === 'toggle'
      ? ['yes', 'no']
      : field.options
    ).map((name, i) => ({ label: name, value: String(i) }));
    const defaultIndex = field.kind === 'toggle' ? 1 : field.options.indexOf(field.def ?? '');
    const items2 = defaultIndex > 0 ? [...items.slice(1), items[0]] : items;
    return (
      <Box flexDirection="column" paddingX={1}>
        {field.help != null && <Text dimColor>{field.label} — {field.help}</Text>}
        {field.help == null && <Text>{field.label}</Text>}
        <SelectInput
          items={items2}
          onSelect={(item) => {
            if (field.kind === 'toggle') setAnswer(item.label === 'yes');
            else setAnswer(field.options[Number(item.value)]);
          }}
        />
        <Text dimColor>  ↑/↓ choose · ↵ next · esc skip-to-confirm · ctrl+c quit</Text>
      </Box>
    );
  }

  // Text/positional/extra-arguments field.
  return (
    <Box flexDirection="column" paddingX={1}>
      {field.help != null && <Text dimColor>{field.label} — {field.help}</Text>}
      {field.help == null && <Text>{field.label}</Text>}
      <Text>  </Text>
      <TextInput
        value={typeof answers[field.id] === 'string' ? answers[field.id] as string : ''}
        placeholder={field.def ?? ''}
        onChange={(v) => setAnswers((a) => ({ ...a, [field.id]: v }))}
        onSubmit={advance}
      />
      <Text dimColor>  ↵ next · esc skip-to-confirm · ctrl+c quit</Text>
    </Box>
  );
}

function buildFields(node: FlatCommand['node']): Field[] {
  const list: Field[] = [];
  for (const f of node.flags) {
    if (f.hidden) continue;
    const label = f.positional ? (f.value_name ?? f.id) : `--${f.long ?? f.short}`;
    if (f.possible_values.length > 0) {
      list.push({ id: f.id, label, help: f.help, kind: 'select', def: f.default_values[0] ?? null, options: f.possible_values.map((pv) => pv.name) });
    } else if (f.takes_value) {
      list.push({ id: f.id, label, help: f.help, kind: 'text', def: f.default_values[0] ?? '', options: [] });
    } else {
      list.push({ id: f.id, label, help: f.help, kind: 'toggle', def: null, options: ['yes', 'no'] });
    }
  }
  if (node.trailing_var_arg) {
    list.push({ id: PASSTHROUGH_KEY, label: 'extra arguments', help: 'Forwarded to the plugin verbatim', def: '', options: [], kind: 'text' });
  }
  list.push({ id: 'verbose', label: '--verbose', help: 'Verbose output', kind: 'toggle', def: null, options: ['yes', 'no'] });
  return list;
}

function initAnswers(fields: Field[]): Answers {
  const init: Answers = {};
  for (const f of fields) {
    if (f.kind === 'toggle') init[f.id] = false;
  }
  for (const f of fields) {
    if (f.kind === 'text' && f.def != null && f.def !== '') init[f.id] = f.def;
  }
  return init;
}
