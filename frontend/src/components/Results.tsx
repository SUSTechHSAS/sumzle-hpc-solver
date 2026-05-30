import type { SolveResponse } from '../types';
import './Results.css';

interface ResultsProps {
  data: SolveResponse | null;
  loading: boolean;
  error: string | null;
}

export default function Results({ data, loading, error }: ResultsProps) {
  if (loading) {
    return (
      <div className="results-section">
        <div className="results-loading">
          <div className="spinner" />
          <span>Solving puzzle...</span>
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="results-section">
        <div className="results-error">
          <span className="error-icon">⚠️</span>
          <span>{error}</span>
        </div>
      </div>
    );
  }

  if (!data) {
    return null;
  }

  const { solutions, stats } = data;

  return (
    <div className="results-section" data-testid="results-section">
      <h2 className="results-title">Results</h2>

      <div className="results-summary">
        Found <strong>{stats.found_count}</strong> solution{stats.found_count !== 1 ? 's' : ''} in{' '}
        <strong>{stats.elapsed_ms}ms</strong>
      </div>

      <div className="stats-grid">
        <div className="stat-card">
          <span className="stat-value">{stats.searched_count.toLocaleString()}</span>
          <span className="stat-label">Expressions Searched</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{stats.found_count.toLocaleString()}</span>
          <span className="stat-label">Solutions Found</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{stats.elapsed_ms}ms</span>
          <span className="stat-label">Time Elapsed</span>
        </div>
        <div className="stat-card">
          <span className="stat-value">{(stats.speed / 1000).toFixed(0)}K</span>
          <span className="stat-label">Expr/sec (×1000)</span>
        </div>
      </div>

      {solutions.length > 0 && (
        <div className="solutions-list">
          <h3>Solutions</h3>
          <div className="solutions-grid">
            {solutions.map((sol, i) => (
              <div key={i} className="solution-card">
                {sol}
              </div>
            ))}
          </div>
        </div>
      )}

      {solutions.length === 0 && (
        <div className="no-solutions">No solutions found for this puzzle.</div>
      )}
    </div>
  );
}
