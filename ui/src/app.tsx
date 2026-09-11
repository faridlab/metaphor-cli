import { useEffect, useState } from 'react';
import { Box, Text } from 'ink';
import Spinner from 'ink-spinner';
import { buildArgv } from './argv.ts';
import { defaultAnswers, needsUserInput } from './defaults.ts';
import { fetchManifest, flattenCommands } from './manifest.ts';
import { fetchOverview, type OverviewData } from './overview.ts';
import { isDestructive } from './safety.ts';
import type { FlatCommand, RunResult } from './types.ts';
import { Overview } from './components/Overview.tsx';
import { Browser } from './components/Browser.tsx';
import { OptionsForm } from './components/OptionsForm.tsx';
import { ConfirmView } from './components/ConfirmView.tsx';
import { RunView } from './components/RunView.tsx';
import { ResultView } from './components/ResultView.tsx';

type Screen =
  | { kind: 'loading' }
  | { kind: 'error'; message: string }
  | { kind: 'overview' }
  | { kind: 'browser' }
  | { kind: 'options'; cmd: FlatCommand }
  | { kind: 'confirm'; cmd: FlatCommand; answers: Record<string, string | boolean> }
  | { kind: 'run'; cmd: FlatCommand; argv: string[] }
  | { kind: 'result'; cmd: FlatCommand; argv: string[]; result: RunResult };

/** argv for running a command straight from the list with every default. */
function runArgv(cmd: FlatCommand): string[] {
  return buildArgv(cmd.node, cmd.path, defaultAnswers(cmd.node));
}

export function App({ launcher }: { launcher: string }) {
  const [screen, setScreen] = useState<Screen>({ kind: 'loading' });
  const [commands, setCommands] = useState<FlatCommand[]>([]);
  const [overview, setOverview] = useState<OverviewData | null>(null);

  useEffect(() => {
    try {
      const manifest = fetchManifest(launcher);
      const cmds = flattenCommands(manifest);
      const ov = fetchOverview(launcher);
      setCommands(cmds);
      setOverview(ov);
      setScreen({ kind: 'overview' });
    } catch (e) {
      setScreen({ kind: 'error', message: (e as Error).message });
    }
  }, [launcher]);

  switch (screen.kind) {
    case 'loading':
      return (
        <Box padding={1}>
          <Spinner type="dots" />
          <Text> loading metaphor manifest…</Text>
        </Box>
      );
    case 'error':
      return (
        <Box padding={1} flexDirection="column">
          <Text color="red">✗ {screen.message}</Text>
          <Text dimColor>  launcher: {launcher}</Text>
          <Text dimColor>  press ctrl+c to exit</Text>
        </Box>
      );
    case 'overview':
      return (
        <Overview
          data={overview}
          onOpen={() => setScreen({ kind: 'browser' })}
        />
      );
    case 'browser':
      return (
        <Browser
          commands={commands}
          onPick={(cmd) => {
            if (needsUserInput(cmd)) {
              setScreen({ kind: 'options', cmd });
            } else if (isDestructive(cmd)) {
              setScreen({ kind: 'confirm', cmd, answers: defaultAnswers(cmd.node) });
            } else {
              setScreen({ kind: 'run', cmd, argv: runArgv(cmd) });
            }
          }}
          onOptions={(cmd) => setScreen({ kind: 'options', cmd })}
          onBack={() => {
            if (overview != null) setScreen({ kind: 'overview' });
          }}
        />
      );
    case 'options':
      return (
        <OptionsForm
          cmd={screen.cmd}
          onBack={() => setScreen({ kind: 'browser' })}
          onSubmit={(answers) => {
            if (isDestructive(screen.cmd)) {
              setScreen({ kind: 'confirm', cmd: screen.cmd, answers });
            } else {
              setScreen({
                kind: 'run',
                cmd: screen.cmd,
                argv: buildArgv(screen.cmd.node, screen.cmd.path, answers),
              });
            }
          }}
        />
      );
    case 'confirm':
      return (
        <ConfirmView
          cmd={screen.cmd}
          answers={screen.answers}
          onBack={() => setScreen({ kind: 'options', cmd: screen.cmd })}
          onRun={(argv) => setScreen({ kind: 'run', cmd: screen.cmd, argv })}
        />
      );
    case 'run':
      return (
        <RunView
          launcher={launcher}
          args={screen.argv}
          onFinish={(result) =>
            setScreen({ kind: 'result', cmd: screen.cmd, argv: screen.argv, result })
          }
        />
      );
    case 'result':
      return (
        <ResultView
          cmd={screen.cmd}
          argv={screen.argv}
          result={screen.result}
          onBack={() => setScreen({ kind: 'browser' })}
          onOptions={() => setScreen({ kind: 'options', cmd: screen.cmd })}
          onRerun={() => setScreen({ kind: 'run', cmd: screen.cmd, argv: screen.argv })}
        />
      );
  }
}
