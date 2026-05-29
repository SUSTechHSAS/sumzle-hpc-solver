import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi } from 'vitest';
import GuessRowComponent, { createBlankRow, cycleState } from './GuessRow';

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
    const inputs = screen.getAllByLabelText('Tile character');
    expect(inputs).toHaveLength(5);
  });

  it('renders the row label', () => {
    const row = createBlankRow(3);
    render(
      <GuessRowComponent
        row={row}
        rowIndex={2}
        onTileCharChange={mockOnCharChange}
        onTileStateToggle={mockOnStateToggle}
      />,
    );
    expect(screen.getByText('Row 3')).toBeInTheDocument();
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
    const inputs = screen.getAllByLabelText('Tile character');
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
    const inputs = screen.getAllByLabelText('Tile character');
    await user.type(inputs[0], '5');
    expect(onCharChange).toHaveBeenCalledWith(0, 0, '5');
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
    const buttons = screen.getAllByLabelText(/State:/);
    await user.click(buttons[0]);
    expect(onStateToggle).toHaveBeenCalledWith(0, 0);
  });
});

describe('createBlankRow', () => {
  it('creates a row with the correct number of empty tiles', () => {
    const row = createBlankRow(5);
    expect(row.tiles).toHaveLength(5);
    row.tiles.forEach((tile) => {
      expect(tile.char).toBe('');
      expect(tile.state).toBe('empty');
    });
  });
});

describe('cycleState', () => {
  it('cycles through correct → present → empty → correct', () => {
    expect(cycleState('correct')).toBe('present');
    expect(cycleState('present')).toBe('empty');
    expect(cycleState('empty')).toBe('correct');
  });
});
