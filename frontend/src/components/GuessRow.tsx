import type { GuessRow as GuessRowType } from '../types';
import TileComponent from './Tile';
import './GuessRow.css';

interface GuessRowProps {
  row: GuessRowType;
  rowIndex: number;
  onTileCharChange: (rowIndex: number, tileIndex: number, char: string) => void;
  onTileStateToggle: (rowIndex: number, tileIndex: number) => void;
}

export default function GuessRow({ row, rowIndex, onTileCharChange, onTileStateToggle }: GuessRowProps) {
  return (
    <div className="guess-row" data-testid={`guess-row-${rowIndex}`}>
      <span className="row-label">Row {rowIndex + 1}</span>
      <div className="tiles-container">
        {row.tiles.map((tile, tileIndex) => (
          <TileComponent
            key={tileIndex}
            tile={tile}
            onCharChange={(char) => onTileCharChange(rowIndex, tileIndex, char)}
            onStateToggle={() => onTileStateToggle(rowIndex, tileIndex)}
          />
        ))}
      </div>
    </div>
  );
}
