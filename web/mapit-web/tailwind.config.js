/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      animation: {
        indeterminate: 'indeterminate 1.4s ease-in-out infinite',
      },
      colors: {
        mapit: {
          bg: '#1f1813',
          surface: '#241d15',
          surface2: '#34291e',
          border: '#4d3d2e',
          text: '#e8ddd0',
          muted: '#9b8b78',
          accent: '#d4a15d',
          success: '#7a9c6a',
          warning: '#d4964a',
          danger: '#c75a4a',
          node: {
            feature: '#d4a15d',
            file: '#7a9c6a',
            function: '#c75a4a',
            module: '#9b7bb8',
            type: '#d4964a',
            macro: '#c77a9a',
            global: '#6aab9e',
            external: '#6d5c4b'
          }
        },
      },
    },
  },
  plugins: [],
}
