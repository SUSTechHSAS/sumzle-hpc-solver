import { useState, useRef } from 'react';
import type { GuessRow, TileState } from '../types';
import './ImportGameState.css';

interface ImportGameStateProps {
  length: number;
  onImport: (length: number, rows: GuessRow[]) => void;
}

interface SumzleRow {
  length?: number;
  rows?: { char: string; state: string }[][];
}

export default function ImportGameState({ length, onImport }: ImportGameStateProps) {
  const [showImport, setShowImport] = useState(false);
  const [importText, setImportText] = useState('');
  const [importError, setImportError] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleImportPaste = () => {
    if (!importText.trim()) return;
    setImportError(null);
    try {
      const data = JSON.parse(importText.trim());
      parseAndImport(data);
      setImportText('');
      setShowImport(false);
    } catch (e) {
      setImportError(e instanceof Error ? e.message : '无效的JSON格式');
    }
  };

  const handleFileImport = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      try {
        const data = JSON.parse(ev.target?.result as string);
        parseAndImport(data);
      } catch (err) {
        setImportError(err instanceof Error ? err.message : '无效的JSON文件');
      }
    };
    reader.readAsText(file);
    // Reset file input
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const parseAndImport = (data: SumzleRow) => {
    // Support format: { length, rows: [[ {char, state} ]] }
    if (!data.rows || !Array.isArray(data.rows)) {
      throw new Error('JSON格式错误: 缺少rows数组');
    }

    const importedLength = data.length || length;
    const rows: GuessRow[] = data.rows.map((row: { char: string; state: string }[]) => {
      const tiles = row.map((tile: { char: string; state: string }) => ({
        char: tile.char || '',
        state: normalizeState(tile.state),
      }));
      return { tiles };
    });

    onImport(importedLength, rows);
  };

  const normalizeState = (state: string): TileState => {
    const s = state.toLowerCase();
    if (s === 'correct' || s === 'green' || s === 'g') return 'correct';
    if (s === 'present' || s === 'yellow' || s === 'y') return 'present';
    return 'empty';
  };

  return (
    <div className="import-section">
      <button
        className="btn btn-import-toggle"
        onClick={() => setShowImport(!showImport)}
      >
        {showImport ? '🔽 收起导入' : '📥 导入局面'}
      </button>

      {showImport && (
        <div className="import-panel">
          <textarea
            className="import-textarea"
            placeholder="粘贴Sumzle游戏状态JSON..."
            value={importText}
            onChange={(e) => setImportText(e.target.value)}
            rows={4}
          />
          <div className="import-actions">
            <button className="btn btn-primary btn-sm" onClick={handleImportPaste}>
              导入JSON
            </button>
            <button
              className="btn btn-secondary btn-sm"
              onClick={() => fileInputRef.current?.click()}
            >
              📂 从文件导入
            </button>
            <input
              ref={fileInputRef}
              type="file"
              accept=".json"
              style={{ display: 'none' }}
              onChange={handleFileImport}
            />
          </div>
          {importError && <div className="import-error">{importError}</div>}
        </div>
      )}
    </div>
  );
}
