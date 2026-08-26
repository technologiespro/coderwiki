import typography from '@tailwindcss/typography';

export default {
  darkMode: 'class',
  content: ['./index.html', './src/**/*.{vue,js}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'Roboto', 'system-ui', '-apple-system', 'sans-serif'],
      },
    },
  },
  plugins: [typography],
};
