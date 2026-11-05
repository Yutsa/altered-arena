import 'react';

// CSS custom properties in style objects ({ '--x': 1 }): typed without a computed key, which the
// React Compiler cannot compile
declare module 'react' {
  interface CSSProperties {
    [key: `--${string}`]: string | number | undefined;
  }
}
