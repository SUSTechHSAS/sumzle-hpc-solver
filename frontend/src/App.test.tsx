import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the title', () => {
    render(<App />);
    expect(screen.getByText('Sumzle Solver')).toBeInTheDocument();
  });

  it('renders the solve button', () => {
    render(<App />);
    expect(screen.getByRole('button', { name: /开始求解/i })).toBeInTheDocument();
  });

  it('renders the length input', () => {
    render(<App />);
    expect(screen.getByLabelText('表达式长度:')).toBeInTheDocument();
  });

  it('renders add row and clear buttons', () => {
    render(<App />);
    expect(screen.getByText('+ 添加行')).toBeInTheDocument();
    expect(screen.getByText('清空')).toBeInTheDocument();
  });

  it('allows entering a multi-digit expression length without clamping the first digit', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:');

    await user.clear(input);
    await user.type(input, '1');
    expect(input).toHaveValue(1);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(5);

    await user.type(input, '1');
    expect(input).toHaveValue(11);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(11);
  });

  it('never renders more than MAX_LENGTH tiles for an extremely large input (issue #29)', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:') as HTMLInputElement;

    await user.clear(input);
    await user.type(input, '1000000');

    // The input field shows what the user typed…
    expect(input).toHaveValue(1000000);
    // …but the rendered tile count is capped so the browser does not freeze.
    const tiles = screen.getAllByLabelText('输入方块字符');
    expect(tiles.length).toBeLessThanOrEqual(64);
    // An accessible error message explains why the typed value is not honored.
    expect(screen.getByRole('alert')).toBeInTheDocument();

    // On blur, the input is normalized to the maximum supported value.
    await user.tab();
    expect(input).toHaveValue(64);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(64);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('accepts the maximum supported expression length without error', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:') as HTMLInputElement;

    await user.clear(input);
    await user.type(input, '64');
    expect(input).toHaveValue(64);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(64);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('rejects length values just above the maximum and surfaces an error', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:') as HTMLInputElement;

    await user.clear(input);
    // Pasting a single out-of-range value in one shot (rather than typing
    // digit-by-digit, which would commit intermediate in-range prefixes).
    await user.paste('65');
    expect(input).toHaveValue(65);
    expect(screen.getByRole('alert')).toBeInTheDocument();
    // No commit happened, so the tile count remains at the default of 5.
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(5);

    // On blur the value is clamped to MAX_LENGTH.
    await user.tab();
    expect(input).toHaveValue(64);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('renders expression evaluator', () => {
    render(<App />);
    expect(screen.getByTestId('expression-evaluator')).toBeInTheDocument();
  });

  it('renders equation validator', () => {
    render(<App />);
    expect(screen.getByTestId('equation-validator')).toBeInTheDocument();
  });

  it('renders a default guess row', () => {
    render(<App />);
    expect(screen.getByTestId('guess-row-0')).toBeInTheDocument();
  });

  it('renders the virtual keyboard', () => {
    render(<App />);
    // Virtual keyboard should have backspace key
    expect(screen.getByText('⌫')).toBeInTheDocument();
  });

  it('renders the import game state button', () => {
    render(<App />);
    expect(screen.getByText('导入局面')).toBeInTheDocument();
  });

  it('renders the theme toggle button', () => {
    render(<App />);
    expect(screen.getByTitle('切换到暗色模式')).toBeInTheDocument();
  });

  it('renders the help section', () => {
    render(<App />);
    expect(screen.getByText('操作提示')).toBeInTheDocument();
  });
});
