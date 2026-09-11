import { useEffect, useState } from 'react';
import { Box, Text } from 'ink';
import Spinner from 'ink-spinner';
import { fetchManifest, flattenCommands } from './manifest.ts';
import { fetchOverview, type OverviewData } from './overview.ts';
import type { Answers, FlatCommand, RunResult } from './types.ts';
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
  | { kind: 'confirm'; cmd: FlatCommand; answers: Answers }
  | { kind: 'run'; cmd: FlatCommand; argv: string[] }
  | { kind: 'result'; cmd: FlatCommand; argv: string[]; result: RunResult };

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
          onPick={(cmd) => setScreen({ kind: 'options', cmd })}
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
          onSubmit={(answers) =>
            setScreen({ kind: 'confirm', cmd: screen.cmd, answers })
          }
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
          onRerun={() => setScreen({ kind: 'run', cmd: screen.cmd, argv: screen.argv })}
        />
      );
  }
}
