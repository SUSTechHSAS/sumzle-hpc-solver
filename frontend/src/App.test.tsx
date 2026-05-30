import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the title', () => {
    render(<App />);
    expect(screen.getByText('Sumzle HPC Solver')).toBeInTheDocument();
  });

  it('renders the subtitle', () => {
    render(<App />);
    expect(screen.getByText('High-performance equation puzzle solver')).toBeInTheDocument();
  });

  it('renders the solve button', () => {
    render(<App />);
    expect(screen.getByRole('button', { name: /solve/i })).toBeInTheDocument();
  });

  it('renders the length input', () => {
    render(<App />);
    expect(screen.getByLabelText('Equation Length:')).toBeInTheDocument();
  });

  it('renders add row and clear all buttons', () => {
    render(<App />);
    expect(screen.getByText('+ Add Row')).toBeInTheDocument();
    expect(screen.getByText('Clear All')).toBeInTheDocument();
  });

  it('renders expression evaluator', () => {
    render(<App />);
    expect(screen.getByTestId('expression-evaluator')).toBeInTheDocument();
  });

  it('renders equation validator', () => {
    render(<App />);
    expect(screen.getByTestId('equation-validator')).toBeInTheDocument();
  });

  it('renders a default guess row', () => {
    render(<App />);
    expect(screen.getByTestId('guess-row-0')).toBeInTheDocument();
  });
});
