import { useState } from 'react';
import { validateEquation } from '../api';
import './EquationValidator.css';

export default function EquationValidator() {
  const [equation, setEquation] = useState('');
  const [valid, setValid] = useState<boolean | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleValidate = async () => {
    if (!equation.trim()) return;
    setLoading(true);
    setError(null);
    setValid(null);
    try {
      const res = await validateEquation(equation.trim());
      setValid(res.valid);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Validation failed');
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleValidate();
  };

  return (
    <div className="validator-section" data-testid="equation-validator">
      <h3>Equation Validator</h3>
      <div className="validator-row">
        <input
          type="text"
          className="validator-input"
          placeholder="e.g. 1+2=3"
          value={equation}
          onChange={(e) => setEquation(e.target.value)}
          onKeyDown={handleKeyDown}
        />
        <button className="validator-btn" onClick={handleValidate} disabled={loading}>
          {loading ? '...' : 'Check'}
        </button>
      </div>
      {valid !== null && (
        <div className={`validator-result ${valid ? 'valid' : 'invalid'}`}>
          {valid ? '✓ Valid equation' : '✗ Invalid equation'}
        </div>
      )}
      {error && <div className="validator-error">{error}</div>}
    </div>
  );
}
