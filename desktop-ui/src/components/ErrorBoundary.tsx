import { Component, ErrorInfo, ReactNode } from 'react';
import logo from '../assets/logo.png';

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  public state: State = {
    hasError: false,
    error: null,
  };

  public static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  public componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('Uncaught error in UI:', error, errorInfo);
  }

  private handleReload = () => {
    window.location.reload();
  };

  private handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  public render() {
    if (this.state.hasError) {
      return (
        <div style={{
          height: '100vh',
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          padding: '2rem',
          textAlign: 'center',
          background: '#0d1117',
          color: '#c9d1d9',
          fontFamily: 'system-ui, -apple-system, BlinkMacSystemFont, sans-serif'
        }}>
          <img src={logo} alt="Baton Logo" style={{ width: '64px', marginBottom: '1.5rem', opacity: 0.8 }} />
          <h2 style={{ fontSize: '1.5rem', fontWeight: 600, color: '#f85149', marginBottom: '0.75rem' }}>
            Something went wrong
          </h2>
          <p style={{ maxWidth: '500px', fontSize: '0.9rem', color: '#8b949e', marginBottom: '1.5rem', lineHeight: 1.5 }}>
            {this.state.error?.message || 'An unexpected rendering error occurred in the desktop companion.'}
          </p>
          <div style={{ display: 'flex', gap: '0.75rem' }}>
            <button
              onClick={this.handleReset}
              style={{
                padding: '0.6rem 1.2rem',
                borderRadius: '8px',
                border: '1px solid rgba(255,255,255,0.1)',
                background: 'rgba(255,255,255,0.05)',
                color: '#c9d1d9',
                cursor: 'pointer',
                fontSize: '0.875rem'
              }}
            >
              Try Again
            </button>
            <button
              onClick={this.handleReload}
              style={{
                padding: '0.6rem 1.2rem',
                borderRadius: '8px',
                border: 'none',
                background: '#238636',
                color: '#ffffff',
                cursor: 'pointer',
                fontWeight: 600,
                fontSize: '0.875rem'
              }}
            >
              Reload Application
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
