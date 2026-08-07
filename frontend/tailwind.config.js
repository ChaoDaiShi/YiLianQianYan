/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        surface: {
          DEFAULT: "var(--bg)",
          2: "var(--bg-2)",
        },
        panel: {
          DEFAULT: "var(--panel)",
          2: "var(--panel-2)",
          hover: "var(--panel-hover)",
        },
        ink: {
          DEFAULT: "var(--text)",
          muted: "var(--text-muted)",
          faint: "var(--text-faint)",
        },
        line: "var(--border)",
        accent: {
          DEFAULT: "var(--accent)",
          fg: "var(--accent-fg)",
        },
        primary: {
          50: "var(--accent)",
          100: "var(--accent)",
          200: "var(--accent)",
          300: "var(--accent)",
          400: "var(--accent)",
          500: "var(--accent)",
          600: "var(--accent)",
          700: "var(--accent)",
          800: "var(--accent)",
          900: "var(--accent)",
        },
      },
      fontFamily: {
        sans: ['"IBM Plex Sans"', "Segoe UI", "PingFang SC", "Microsoft YaHei", "sans-serif"],
        mono: ['"IBM Plex Mono"', "Cascadia Code", "Consolas", "ui-monospace", "monospace"],
      },
      animation: {
        "pulse-dot": "pulse-dot 1.4s infinite ease-in-out",
      },
      keyframes: {
        "pulse-dot": {
          "0%, 80%, 100%": { opacity: "0" },
          "40%": { opacity: "1" },
        },
      },
    },
  },
  plugins: [],
};
