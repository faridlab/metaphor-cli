import { describe, expect, it } from 'vitest';
import { render } from 'ink-testing-library';
import React from 'react';
import { Browser } from '../src/components/Browser.tsx';
import { flattenCommands } from '../src/manifest.ts';
import { FIXTURE_MANIFEST } from './fixtures.ts';

describe('Browser smoke render', () => {
  it('renders grouped commands off-screen (no TTY needed)', () => {
    const commands = flattenCommands(FIXTURE_MANIFEST);
    const { lastFrame } = render(
      <Browser commands={commands} onPick={() => {}} onBack={() => {}} />,
    );
    const frame = lastFrame() ?? '';
    expect(frame).toContain('Commands');
    expect(frame).toContain('info');
    expect(frame).toContain('agent');
  });
});
