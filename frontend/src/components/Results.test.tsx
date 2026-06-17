import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import Results from './Results';
import type { SolveResponse } from '../types';

describe('Results', () => {
  const mockOnDownload = vi.fn();

  it('shows placeholder when no data', () => {
    render(<Results data={null} loading={false} error={null} onDownload={mockOnDownload}  />);
    expect(screen.getByText('输入反馈后运行求解，结果会显示在这里。')).toBeInTheDocument();
  });

  it('shows loading state', () => {
    render(<Results data={null} loading={true} error={null} onDownload={mockOnDownload}  />);
    expect(screen.getByText('正在搜索候选表达式…')).toBeInTheDocument();
  });

  it('shows error message', () => {
    render(<Results data={null} loading={false} error="Something went wrong" onDownload={mockOnDownload}  />);
    expect(screen.getByText('Something went wrong')).toBeInTheDocument();
  });

  it('offers retry from the error state when a retry handler is provided', async () => {
    const user = userEvent.setup();
    const onRetry = vi.fn();
    render(
      <Results
        data={null}
        loading={false}
        error="求解失败：网络中断"
        onDownload={mockOnDownload}
        onRetry={onRetry}
      />,
    );

    await user.click(screen.getByRole('button', { name: '重新求解' }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('displays solutions and stats', () => {
    const data: SolveResponse = {
      solutions: ['1+2=3', '2+3=5'],
      stats: { searched_count: 1000, found_count: 2, elapsed_ms: 50, speed: 20000 },
      char_probabilities: [
        { char: '=', display: '=', count: 2, probability: 100 },
        { char: '1', display: '1', count: 1, probability: 50 },
      ],
      recommended: '1+2=3',
      top: 0,
      scores: [],
    };
    const { container } = render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    // Check that solutions are displayed (may appear multiple times: recommended + list)
    expect(container.querySelectorAll('.result-item')).toHaveLength(2);
    expect(container.querySelector('.result-item.recommended')).toBeInTheDocument();
    // Check stats
    expect(screen.getByText('1,000')).toBeInTheDocument();
  });

  it('shows character probabilities', () => {
    const data: SolveResponse = {
      solutions: ['1+2=3'],
      stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
      char_probabilities: [
        { char: '=', display: '=', count: 1, probability: 100 },
        { char: '*', display: '×', count: 1, probability: 100 },
      ],
      recommended: '1+2=3',
      top: 0,
      scores: [],
    };
    const { container } = render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    expect(container.querySelector('.char-prob-section')).toBeInTheDocument();
    expect(screen.getByText('×')).toBeInTheDocument();
  });

  it('shows recommended solution', () => {
    const data: SolveResponse = {
      solutions: ['1+2=3', '2+3=5'],
      stats: { searched_count: 1000, found_count: 2, elapsed_ms: 50, speed: 20000 },
      char_probabilities: [],
      recommended: '1+2=3',
      top: 0,
      scores: [],
    };
    const { container } = render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    expect(container.querySelector('.recommended-section')).toBeInTheDocument();
    expect(container.querySelector('.recommended-expression')).toHaveTextContent('1+2=3');
  });

  it('shows no solutions message when empty', () => {
    const data: SolveResponse = {
      solutions: [],
      stats: { searched_count: 100, found_count: 0, elapsed_ms: 5, speed: 20000 },
      char_probabilities: [],
      recommended: null,
      top: 0,
      scores: [],
    };
    render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    expect(screen.getByText('没有符合当前反馈的解。请检查颜色状态或表达式长度。')).toBeInTheDocument();
  });

  it('shows download buttons when solutions exist', () => {
    const data: SolveResponse = {
      solutions: ['1+2=3'],
      stats: { searched_count: 100, found_count: 1, elapsed_ms: 5, speed: 20000 },
      char_probabilities: [],
      recommended: '1+2=3',
      top: 0,
      scores: [],
    };
    render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    expect(screen.getByText('JSON')).toBeInTheDocument();
    expect(screen.getByText('CSV')).toBeInTheDocument();
    expect(screen.getByText('TXT')).toBeInTheDocument();
  });

  it('renders per-solution scores and a Top-N badge in top-N mode', () => {
    const data: SolveResponse = {
      solutions: ['1+2=3', '2+3=5'],
      stats: { searched_count: 1000, found_count: 2, elapsed_ms: 50, speed: 20000 },
      char_probabilities: [],
      recommended: '1+2=3',
      top: 2,
      scores: [245.5, 180.25],
    };
    const { container } = render(<Results data={data} loading={false} error={null} onDownload={mockOnDownload}  />);

    // Top-N badge present and scores shown (rounded to one decimal).
    expect(screen.getByText('Top-2')).toBeInTheDocument();
    const scoreEls = container.querySelectorAll('.result-score');
    expect(scoreEls).toHaveLength(2);
    expect(scoreEls[0]).toHaveTextContent('245.5');
    expect(scoreEls[1]).toHaveTextContent('180.3');
  });
});
