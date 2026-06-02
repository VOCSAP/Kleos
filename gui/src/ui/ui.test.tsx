import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { Badge } from './Badge';
import { StatPill } from './StatPill';

describe('ui primitives', () => {
  it('renders a badge label', () => {
    render(<Badge label="active" />);

    expect(screen.getByText('active')).toBeInTheDocument();
  });

  it('renders a stat value and label', () => {
    render(<StatPill value="12" label="tasks" />);

    expect(screen.getByText('12')).toBeInTheDocument();
    expect(screen.getByText('tasks')).toBeInTheDocument();
  });
});
