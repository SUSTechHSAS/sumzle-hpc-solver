import { useState, useCallback } from 'react';
import type { GuessRow, SolveResponse, TileState } from './types';
import { solvePuzzle } from './api';
import GuessRowComponent from './components/GuessRow';
import { createBlankRow, cycleState } from './utils';
import Results from './components/Results';
import ExpressionEvaluator from './components/ExpressionEvaluator';
import EquationValidator from './components/EquationValidator';
import './App.css';

const DEFAULT_LENGTH = 5;

export default function App() {
  const [length, setLength] = useState(DEFAULT_LENGTH);
  const [rows, setRows] = useState<GuessRow[]>([createBlankRow(DEFAULT_LENGTH)]);
  const [solutions, setSolutions] = useState<SolveResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleLengthChange = useCallback((newLength: number) => {
    const clamped = Math.min(8, Math.max(3, newLength));
    setLength(clamped);
    setRows((prev) => prev.map((row) => adjustRowLength(row, clamped)));
    setSolutions(null);
    setError(null);
  }, []);

  const adjustRowLength = (row: GuessRow, len: number): GuessRow => {
    const tiles = [...row.tiles];
    while (tiles.length < len) {
      tiles.push({ char: '', state: 'empty' as TileState });
    }
    while (tiles.length > len) {
      tiles.pop();
    }
    return { tiles };
  };

  const handleTileCharChange = useCallback(
    (rowIndex: number, tileIndex: number, char: string) => {
      setRows((prev) => {
        const newRows = [...prev];
        const newRow = { ...newRows[rowIndex], tiles: [...newRows[rowIndex].tiles] };
        newRow.tiles[tileIndex] = { ...newRow.tiles[tileIndex], char };
        newRows[rowIndex] = newRow;
        return newRows;
      });
      setSolutions(null);
    },
    [],
  );

  const handleTileStateToggle = useCallback((rowIndex: number, tileIndex: number) => {
    setRows((prev) => {
      const newRows = [...prev];
      const newRow = { ...newRows[rowIndex], tiles: [...newRows[rowIndex].tiles] };
      const current = newRow.tiles[tileIndex];
      newRow.tiles[tileIndex] = { ...current, state: cycleState(current.state) };
      newRows[rowIndex] = newRow;
      return newRows;
    });
    setSolutions(null);
  }, []);

  const addRow = useCallback(() => {
    setRows((prev) => [...prev, createBlankRow(length)]);
  }, [length]);

  const removeRow = useCallback(() => {
    setRows((prev) => (prev.length > 1 ? prev.slice(0, -1) : prev));
  }, []);

  const clearAll = useCallback(() => {
    setRows([createBlankRow(length)]);
    setSolutions(null);
    setError(null);
  }, [length]);

  const handleSolve = useCallback(async () => {
    setLoading(true);
    setError(null);
    setSolutions(null);
    try {
      const res = await solvePuzzle(length, rows);
      setSolutions(res);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'An error occurred');
    } finally {
      setLoading(false);
    }
  }, [length, rows]);

  return (
    <div className="app">
      <header className="app-header">
        <h1 className="app-title">Sumzle HPC Solver</h1>
        <p className="app-subtitle">High-performance equation puzzle solver</p>
      </header>

      <main className="app-main">
        <section className="puzzle-section">
          <div className="puzzle-controls">
            <div className="length-control">
              <label htmlFor="length-input">Equation Length:</label>
              <input
                id="length-input"
                type="number"
                min={3}
                max={8}
                value={length}
                onChange={(e) => handleLengthChange(parseInt(e.target.value, 10) || 3)}
                className="length-input"
              />
            </div>
            <div className="row-buttons">
              <button className="btn btn-secondary" onClick={addRow}>
                + Add Row
              </button>
              <button className="btn btn-secondary" onClick={removeRow} disabled={rows.length <= 1}>
                − Remove Row
              </button>
              <button className="btn btn-danger" onClick={clearAll}>
                Clear All
              </button>
            </div>
          </div>

          <div className="guess-rows">
            {rows.map((row, i) => (
              <GuessRowComponent
                key={i}
                row={row}
                rowIndex={i}
                onTileCharChange={handleTileCharChange}
                onTileStateToggle={handleTileStateToggle}
              />
            ))}
          </div>

          <div className="solve-section">
            <button
              className="btn btn-primary btn-solve"
              onClick={handleSolve}
              disabled={loading}
            >
              {loading ? 'Solving...' : '🧩 Solve'}
            </button>
            <p className="solve-hint">
              Click the corner button on each tile to cycle its color: green (correct position) →
              yellow (wrong position) → gray (absent)
            </p>
          </div>
        </section>

        <Results data={solutions} loading={loading} error={error} />

        <section className="tools-section">
          <div className="tools-grid">
            <ExpressionEvaluator />
            <EquationValidator />
          </div>
        </section>
      </main>

      <footer className="app-footer">
        <p>
          Sumzle HPC Solver &mdash; Powered by Rust + axum backend
        </p>
      </footer>
    </div>
  );
}
