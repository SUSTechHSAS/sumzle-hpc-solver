import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import Results from './Results';
import type { SolveResponse } from '../types';

const mockData: SolveResponse = {
  solutions: ['1+2=3', '2+1=3', '3-2=1'],
  stats: {
    searched_count: 841,
    found_count: 3,
    elapsed_ms: 1,
    speed: 841000,
  },
};

describe('Results', () => {
  it('renders nothing when data is null and not loading', () => {
    const { container } = render(<Results data={null} loading={false} error={null} />);
    expect(container.firstChild).toBeNull();
  });

  it('renders loading state', () => {
    render(<Results data={null} loading={true} error={null} />);
    expect(screen.getByText('Solving puzzle...')).toBeInTheDocument();
  });

  it('renders error state', () => {
    render(<Results data={null} loading={false} error="Something went wrong" />);
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
  });

  it('renders results with stats', () => {
    render(<Results data={mockData} loading={false} error={null} />);
    expect(screen.getByText('Results')).toBeInTheDocument();
    expect(screen.getByText('841')).toBeInTheDocument();
    // "3" appears in multiple places, use getAllByText
    const threes = screen.getAllByText('3');
    expect(threes.length).toBeGreaterThanOrEqual(1);
  });

  it('renders solution cards', () => {
    render(<Results data={mockData} loading={false} error={null} />);
    expect(screen.getByText('1+2=3')).toBeInTheDocument();
    expect(screen.getByText('2+1=3')).toBeInTheDocument();
    expect(screen.getByText('3-2=1')).toBeInTheDocument();
  });

  it('renders summary section', () => {
    render(<Results data={mockData} loading={false} error={null} />);
    // Check for the summary container which contains the text split across elements
    const summary = document.querySelector('.results-summary');
    expect(summary).toBeInTheDocument();
    expect(summary?.textContent).toContain('Found');
    expect(summary?.textContent).toContain('3');
    expect(summary?.textContent).toContain('1ms');
  });

  it('renders no solutions message when empty', () => {
    const noSolutionsData: SolveResponse = {
      solutions: [],
      stats: { searched_count: 100, found_count: 0, elapsed_ms: 5, speed: 20000 },
    };
    render(<Results data={noSolutionsData} loading={false} error={null} />);
    expect(screen.getByText('No solutions found for this puzzle.')).toBeInTheDocument();
  });
});
