import { type Tile as TileType, type TileState } from '../types';
import './Tile.css';

interface TileProps {
  tile: TileType;
  onCharChange: (char: string) => void;
  onStateToggle: () => void;
}

const STATE_LABELS: Record<TileState, string> = {
  correct: '✓',
  present: '~',
  empty: '✗',
};

export default function Tile({ tile, onCharChange, onStateToggle }: TileProps) {
  const handleInput = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = e.target.value;
    if (val.length === 0) {
      onCharChange('');
    } else {
      onCharChange(val.slice(-1));
    }
  };

  return (
    <div className={`tile tile-${tile.state}`}>
      <input
        className="tile-input"
        type="text"
        maxLength={1}
        value={tile.char}
        onChange={handleInput}
        aria-label="Tile character"
      />
      <button
        className={`tile-state-btn tile-state-${tile.state}`}
        onClick={onStateToggle}
        title={`State: ${tile.state}. Click to toggle.`}
        aria-label={`State: ${tile.state}`}
      >
        {STATE_LABELS[tile.state]}
      </button>
    </div>
  );
}

export { STATE_LABELS };
