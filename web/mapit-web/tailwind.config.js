/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  theme: {
    extend: {
      colors: {
        mapit: {
          bg: '#0b0d12',
          surface: '#14171f',
          surface2: '#1b1f2a',
          border: '#262b38',
          text: '#e8eaf0',
          muted: '#8b91a3',
          accent: '#5b8def',
          success: '#3ecf8e',
          warning: '#e0a440',
          danger: '#e5566d',
          node: {
            feature: '#5b8def',
            file: '#3ecf8e',
            function: '#e5566d',
            module: '#a684e8',
            type: '#e0a440',
            macro: '#c792ea',
            global: '#4fc3d9',
            external: '#5c6577'
          }
        },
      },
    },
  },
  plugins: [],
}
