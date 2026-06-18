import type { CharProbability } from '../types';
import Icon from './Icon';
import './CharacterProbability.css';

interface CharacterProbabilityProps {
  probabilities: CharProbability[];
}

export default function CharacterProbability({ probabilities }: CharacterProbabilityProps) {
  if (probabilities.length === 0) {
    return null;
  }

  const maxProb = Math.max(...probabilities.map((p) => p.probability));

  return (
    <div className="char-prob-section">
      <h3 className="section-title"><Icon name="key" />字符概率</h3>
      <div className="char-prob-container">
        {probabilities.map((p) => (
          <div key={p.char} className="prob-item">
            <span className="prob-char-display">{p.display}</span>
            <div className="prob-bar-container">
              <div
                className="prob-bar"
                style={{
                  width: '100%',
                  transform: `scaleX(${maxProb > 0 ? p.probability / maxProb : 0})`,
                }}
              />
            </div>
            <span className="prob-value">{p.probability.toFixed(1)}%</span>
          </div>
        ))}
      </div>
    </div>
  );
}
