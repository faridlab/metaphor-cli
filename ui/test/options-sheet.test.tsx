import { describe, expect, it, vi } from 'vitest';
import { render } from 'ink-testing-library';
import React from 'react';
import { OptionsForm } from '../src/components/OptionsForm.tsx';
import { INFO_CMD } from './fixtures.ts';

// ink parses one keypress per stdin data event; pause between writes so the
// test keys arrive as separate chunks.
const wait = (ms = 30): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

describe('OptionsForm options sheet', () => {
  it('renders every flag as a row with a live command preview', async () => {
    const { lastFrame } = render(<OptionsForm cmd={INFO_CMD} onBack={() => {}} onSubmit={() => {}} />);
    await wait();
    const frame = lastFrame() ?? '';
    expect(frame).toContain('Options — info');
    expect(frame).toContain('--json');
    expect(frame).toContain('--verbose');
    expect(frame).toContain('Run command');
    expect(frame).toContain('metaphor info');
  });

  it('flips a toggle with space, then submits from the run row', async () => {
    const onSubmit = vi.fn();
    const { stdin } = render(<OptionsForm cmd={INFO_CMD} onBack={() => {}} onSubmit={onSubmit} />);
    await wait();
    stdin.write(' ');  // flip --json (first row)
    await wait();
    stdin.write('j');  // move to --verbose
    await wait();
    stdin.write('j');  // move to Run command
    await wait();
    stdin.write('\r'); // run
    await wait();
    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(onSubmit).toHaveBeenCalledWith({ json: true, verbose: false });
  });

  it('esc goes back without submitting', async () => {
    const onBack = vi.fn();
    const onSubmit = vi.fn();
    const { stdin } = render(<OptionsForm cmd={INFO_CMD} onBack={onBack} onSubmit={onSubmit} />);
    await wait();
    stdin.write('\x1b');
    await wait();
    expect(onBack).toHaveBeenCalledTimes(1);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it('shows a duplicated flag row only once', async () => {
    const doubled = {
      ...INFO_CMD.node,
      flags: [...INFO_CMD.node.flags, INFO_CMD.node.flags[INFO_CMD.node.flags.length - 1]!],
    };
    const { lastFrame } = render(<OptionsForm cmd={{ node: doubled, path: ['info'] }} onBack={() => {}} onSubmit={() => {}} />);
    await wait();
    const frame = lastFrame() ?? '';
    expect(frame.split('--verbose').length - 1).toBe(1);
  });
});
