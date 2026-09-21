/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        brand: {
          50: "#e6fff9",
          100: "#b3ffe9",
          200: "#80ffd9",
          300: "#4dffc9",
          400: "#1affb9",
          500: "#00d4aa",
          600: "#00b894",
          700: "#009980",
          800: "#007a6a",
          900: "#005c50",
        },
      },
      fontFamily: {
        sans: ["Inter", "Vazirmatn", "Segoe UI", "system-ui", "sans-serif"],
      },
      animation: {
        "pulse-slow": "pulse 3s cubic-bezier(0.4, 0, 0.6, 1) infinite",
      },
    },
  },
  plugins: [],
};
