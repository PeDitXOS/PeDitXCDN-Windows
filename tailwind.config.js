/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        brand: {
          50: "#eef8ff",
          100: "#d9efff",
          200: "#bce4ff",
          300: "#8ed5ff",
          400: "#59bdff",
          500: "#21a9ff",
          600: "#0b8ae0",
          700: "#006ebd",
          800: "#065b9a",
          900: "#0b4c7e",
        },
      },
      // G HUB panels are ~8-10px, not Tailwind's 4px. One override covers
      // every `rounded` in the JSX instead of a class sweep.
      borderRadius: {
        DEFAULT: "10px",
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
