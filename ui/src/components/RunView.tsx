import { useEffect, useRef, useState } from 'react';
import { Box, Static, Text, useInput } from 'ink';
import Spinner from 'ink-spinner';
import { runCommand, type RunResult } from '../runner.ts';

let seq = 0;

export function RunView({ launcher, args, onFinish }: {
  launcher: string;
  args: string[];
  onFinish: (result: RunResult) => void;
}): React.JSX.Element {
  const [lines, setLines] = useState<Array<{ id: number; text: string }>>([]);
  const [cancelling, setCancelling] = useState(false);
  const handleRef = useRef<ReturnType<typeof runCommand> | null>(null);
  const finishedRef = useRef(false);

  const push = (text: string): void => {
    const chunks = text.split('\n');
    if (chunks[0] !== '') {
      setLines((prev) => [...prev, { id: seq++, text: chunks[0] }]);
    }
    for (const c of chunks.slice(1)) {
      setLines((prev) => [...prev, { id: seq++, text: c }]);
    }
  };

  useEffect(() => {
    const h = runCommand({
      launcher,
      args,
      cwd: process.cwd(),
      onStdout: push,
      onStderr: push,
    });
    handleRef.current = h;
    h.result.then((r) => {
      finishedRef.current = true;
      onFinish(r);
    });
    return () => {
      handleRef.current?.cancel();
    };
  }, []);

  useInput((input, key) => {
    if (key.ctrl && input === 'c' && !cancelling) {
      setCancelling(true);
      handleRef.current?.cancel();
    }
  });

  return (
    <Box flexDirection="column">
      <Static items={lines}>
        {(line) => (
          <Text key={line.id}> {line.text}</Text>
        )}
      </Static>
      <Box>
        <Spinner type="dots" />
        <Text> running metaphor {args.join(' ')}{cancelling ? ' — cancelling…' : ''}</Text>
      </Box>
    </Box>
  );
}
