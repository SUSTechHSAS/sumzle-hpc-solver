import { useState } from 'react';
import { evaluateExpression } from '../api';
import './ExpressionEvaluator.css';

export default function ExpressionEvaluator() {
  const [expression, setExpression] = useState('');
  const [result, setResult] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleEvaluate = async () => {
    if (!expression.trim()) return;
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const res = await evaluateExpression(expression.trim());
      setResult(res.result);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Evaluation failed');
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleEvaluate();
  };

  return (
    <div className="eval-section" data-testid="expression-evaluator">
      <h3>Expression Evaluator</h3>
      <div className="eval-row">
        <input
          type="text"
          className="eval-input"
          placeholder="e.g. 5! or 3+4*2"
          value={expression}
          onChange={(e) => setExpression(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button className="eval-btn" onClick={handleEvaluate} disabled={loading}>
          {loading ? '...' : '='}
        </button>
      </div>
      {result !== null && (
        <div className="eval-result">
          = <strong>{result}</strong>
        </div>
      )}
      {result === null && error === null && !loading && expression && (
        <div className="eval-result eval-no-result">No result</div>
      )}
      {error && <div className="eval-error">{error}</div>}
    </div>
  );
}
