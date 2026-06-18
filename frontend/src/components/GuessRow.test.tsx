import { fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import GuessRowComponent from './GuessRow';
import { createBlankRow } from '../utils';

describe('GuessRow', () => {
  const mockOnCharChange = vi.fn();
  const mockOnStateToggle = vi.fn();

  it('renders tiles for the row', () => {
    const row = createBlankRow(5);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    // Should have 5 tile inputs
    const inputs = screen.getAllByLabelText('输入方块字符');
    expect(inputs).toHaveLength(5);
  });

  it('renders the row label with Chinese text', () => {
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={2}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    expect(screen.getByText('第 3 行')).toBeInTheDocument();
  });

  it('displays characters in tiles', () => {
    const row = {
      tiles: [
        { char: '1', state: 'correct' as const },
        { char: '+', state: 'present' as const },
        { char: '2', state: 'empty' as const },
      ],
    };
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    const inputs = screen.getAllByLabelText('输入方块字符');
    expect(inputs[0]).toHaveValue('1');
    expect(inputs[1]).toHaveValue('+');
    expect(inputs[2]).toHaveValue('2');
  });

  it('applies correct state class to tiles', () => {
    const row = {
      tiles: [
        { char: '1', state: 'correct' as const },
        { char: '+', state: 'present' as const },
        { char: '2', state: 'empty' as const },
      ],
    };
    const { container } = render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    const tiles = container.querySelectorAll('.tile');
    expect(tiles[0]).toHaveClass('tile-correct');
    expect(tiles[1]).toHaveClass('tile-present');
    expect(tiles[2]).toHaveClass('tile-empty');
  });

  it('calls onTileCharChange when character is typed', async () => {
    const user = userEvent.setup();
    const onCharChange = vi.fn();
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={onCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    const inputs = screen.getAllByLabelText('输入方块字符');
    await user.type(inputs[0], '5');
    expect(onCharChange).toHaveBeenCalledWith(0, 0, '5');
  });

  it('normalizes display operators typed directly into a tile', async () => {
    const user = userEvent.setup();
    const onCharChange = vi.fn();
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={onCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    const inputs = screen.getAllByLabelText('输入方块字符');
    await user.type(inputs[0], '×');
    expect(onCharChange).toHaveBeenCalledWith(0, 0, '*');
  });

  it('rejects unsupported characters typed directly into an empty tile', async () => {
    const user = userEvent.setup();
    const onCharChange = vi.fn();
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={onCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    const inputs = screen.getAllByLabelText('输入方块字符');
    await user.type(inputs[0], 'z');
    expect(onCharChange).toHaveBeenCalledWith(0, 0, '');
  });

  it('keeps an existing tile value when unsupported characters are typed', () => {
    const onCharChange = vi.fn();
    const row = {
      tiles: [
        { char: '5', state: 'empty' as const },
        { char: '', state: 'empty' as const },
        { char: '', state: 'empty' as const },
      ],
    };
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={onCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );

    const inputs = screen.getAllByLabelText('输入方块字符');
    fireEvent.change(inputs[0], { target: { value: '5z' } });
    expect(onCharChange).not.toHaveBeenCalled();
  });

  it('calls onTileStateToggle when state button is clicked', async () => {
    const user = userEvent.setup();
    const onStateToggle = vi.fn();
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={onStateToggle}
      />,
    );
    const buttons = screen.getAllByLabelText(/当前标记：/);
    await user.click(buttons[0]);
    expect(onStateToggle).toHaveBeenCalledWith(0, 0);
  });

  it('supports tile selection', () => {
    const onSelect = vi.fn();
    const row = createBlankRow(3);
    const { container } = render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
        selectedTile={{ row: 0, col: 1 }}
        onTileSelect={onSelect}
      />,
    );
    const tiles = container.querySelectorAll('.tile');
    expect(tiles[1]).toHaveClass('tile-selected');
    expect(tiles[0]).not.toHaveClass('tile-selected');
  });

  it('selects the tile when its input receives focus', () => {
    const onSelect = vi.fn();
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={0}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
        onTileSelect={onSelect}
      />,
    );

    const inputs = screen.getAllByLabelText('输入方块字符');
    fireEvent.focus(inputs[2]);
    expect(onSelect).toHaveBeenCalledWith(0, 2);
  });
});
