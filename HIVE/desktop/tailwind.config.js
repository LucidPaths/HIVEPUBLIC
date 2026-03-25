/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        hive: {
          bg: '#1a1a1a',
          surface: '#2d2d2d',
          border: '#444444',
          accent: '#00ff88',
          'accent-dim': '#00cc6a',
          text: '#e0e0e0',
          'text-dim': '#888888',
          user: '#2d4a7c',
          error: '#ff4444',
          warning: '#ffaa00',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['JetBrains Mono', 'Consolas', 'monospace'],
      },
    },
  },
  plugins: [],
};
