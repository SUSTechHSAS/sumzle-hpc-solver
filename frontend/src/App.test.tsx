import { render, screen, fireEvent } from '@testing-library/react';
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
    // The input is flagged as invalid for assistive tech, and wired to the
    // error message via aria-describedby.
    expect(input).toHaveAttribute('aria-invalid', 'true');
    expect(input).toHaveAttribute('aria-describedby', 'length-error');

    // On blur, the input is normalized to the maximum supported value.
    await user.tab();
    expect(input).toHaveValue(64);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(64);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    // React renders aria-invalid={false} as the literal "false"; the error
    // state is cleared.
    expect(input).toHaveAttribute('aria-invalid', 'false');
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

  it('surfaces an error for zero and negative lengths and recovers on blur', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:') as HTMLInputElement;

    // 0 can never become a valid length by appending digits, so it should
    // error immediately rather than being swallowed silently.
    await user.clear(input);
    await user.paste('0');
    expect(input).toHaveValue(0);
    expect(screen.getByRole('alert')).toBeInTheDocument();
    // Board unchanged (default 5 tiles).
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(5);

    // Same for a negative value.
    await user.clear(input);
    await user.paste('-5');
    expect(input).toHaveValue(-5);
    expect(screen.getByRole('alert')).toBeInTheDocument();
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(5);

    // Blur normalizes to MIN_LENGTH (3) via clampLength.
    await user.tab();
    expect(input).toHaveValue(3);
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(3);
  });

  it('keeps the row buttons on the same line when a length error is shown (issue #31, #33)', async () => {
    const user = userEvent.setup();
    render(<App />);
    const input = screen.getByLabelText('表达式长度:') as HTMLInputElement;

    await user.clear(input);
    await user.paste('65');
    const error = screen.getByRole('alert');
    // The error is a direct child of .puzzle-controls placed AFTER
    // .row-buttons, with width:100% so it takes its own line below
    // the input + buttons row. This keeps the buttons on line 1 (no shift)
    // while letting the error span the full panel width (no narrow wrap).
    const addRowBtn = screen.getByText('+ 添加行');
    const puzzleControls = addRowBtn.closest('.puzzle-controls') as HTMLElement | null;
    expect(puzzleControls).not.toBeNull();
    expect(puzzleControls!).toContainElement(addRowBtn);
    expect(puzzleControls!).toContainElement(error);
    // .length-control holds only the label + input now (not the error).
    const lengthControl = input.closest('.length-control') as HTMLElement | null;
    expect(lengthControl).not.toBeNull();
    expect(lengthControl!).not.toContainElement(error);
    // All three buttons still render (none get pushed off or hidden).
    expect(screen.getByText('+ 添加行')).toBeInTheDocument();
    expect(screen.getByText('− 删除行')).toBeInTheDocument();
    expect(screen.getByText('清空')).toBeInTheDocument();
  });

  it('rejects an imported game state with an empty rows array', async () => {
    const user = userEvent.setup();
    render(<App />);

    // Open the import panel and paste a payload whose rows array is empty.
    await user.click(screen.getByText('导入局面'));
    const textarea = screen.getByPlaceholderText('粘贴 Sumzle 游戏状态 JSON');
    // fireEvent.change is the reliable way to set a React-controlled
    // textarea's value in jsdom (userEvent.type parses `{` as a keyboard
    // modifier, and paste needs clipboard support jsdom lacks).
    fireEvent.change(textarea, { target: { value: '{"length": 5, "rows": []}' } });
    await user.click(screen.getByText('导入JSON'));

    // The import error is surfaced (role="alert" was added to .import-error
    // during polish so it's announced to assistive tech).
    expect(screen.getByRole('alert')).toHaveTextContent('rows数组不能为空');

    // The board is untouched: still one default row of 5 tiles.
    expect(screen.getAllByTestId(/guess-row-/)).toHaveLength(1);
    expect(screen.getAllByLabelText('输入方块字符')).toHaveLength(5);
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
