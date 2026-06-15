import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import App from './App';

describe('App', () => {
  it('renders the title', () => {
    render(<App />);
    expect(screen.getByText('Sumzle Solver')).toBeInTheDocument();
  });

  it('renders the solve button', () => {
    render(<App />);
    expect(screen.getByRole('button', { name: /开始求解/i })).toBeInTheDocument();
  });

  it('renders the length input', () => {
    render(<App />);
    expect(screen.getByLabelText('表达式长度:')).toBeInTheDocument();
  });

  it('renders the threads input', () => {
    render(<App />);
    expect(screen.getByLabelText('线程数:')).toBeInTheDocument();
  });

  it('renders the top-N input', () => {
    render(<App />);
    expect(screen.getByLabelText('Top-N:')).toBeInTheDocument();
  });

  it('renders add row and clear buttons', () => {
    render(<App />);
    expect(screen.getByText('+ 添加行')).toBeInTheDocument();
    expect(screen.getByText('清空')).toBeInTheDocument();
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

  it('renders the virtual keyboard', () => {
    render(<App />);
    // Virtual keyboard should have backspace key
    expect(screen.getByText('⌫')).toBeInTheDocument();
  });

  it('renders the import game state button', () => {
    render(<App />);
    expect(screen.getByText('📥 导入局面')).toBeInTheDocument();
  });

  it('renders the theme toggle button', () => {
    render(<App />);
    expect(screen.getByTitle('切换到暗色模式')).toBeInTheDocument();
  });

  it('renders the help section with threads and top-N tips', () => {
    render(<App />);
    expect(screen.getByText('💡 操作提示')).toBeInTheDocument();
    expect(screen.getByText(/线程数：0/)).toBeInTheDocument();
    expect(screen.getByText(/Top-N：0/)).toBeInTheDocument();
  });

  it('defaults threads and top-N inputs to 0', () => {
    render(<App />);
    const threadsInput = screen.getByLabelText('线程数:') as HTMLInputElement;
    const topNInput = screen.getByLabelText('Top-N:') as HTMLInputElement;
    expect(threadsInput.value).toBe('0');
    expect(topNInput.value).toBe('0');
  });
});
