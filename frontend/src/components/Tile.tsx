import { type Tile as TileType, type TileState } from '../types';
import './Tile.css';

interface TileProps {
  tile: TileType;
  onCharChange: (char: string) => void;
  onStateToggle: () => void;
  selected?: boolean;
  onSelect?: () => void;
}

const STATE_LABELS: Record<TileState, string> = {
  correct: '✓',
  present: '●',
  empty: '✕',
};

export default function Tile({ tile, onCharChange, onStateToggle, selected, onSelect }: TileProps) {
  const handleInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    if (val.length === 0) {
      onCharChange('');
    } else {
      onCharChange(val.slice(-1));
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Backspace' && tile.char === '') {
      onCharChange('');
    }
  };

  return (
    <div
      className={`tile tile-${tile.state}${selected ? ' tile-selected' : ''}`}
      onClick={onSelect}
    >
      <input
        className="tile-input"
        type="text"
        maxLength={1}
        value={tile.char}
        onChange={handleInput}
        onKeyDown={handleKeyDown}
        aria-label="Tile character"
        onClick={(e) => e.stopPropagation()}
      />
      <button
        className={`tile-state-btn tile-state-${tile.state}`}
        onClick={(e) => {
          e.stopPropagation();
          onStateToggle();
        }}
        title={`State: ${tile.state}. Click to toggle.`}
        aria-label={`State: ${tile.state}`}
      >
        {STATE_LABELS[tile.state]}
      </button>
    </div>
  );
}

export { STATE_LABELS };
