/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        canvas: "var(--ds-bg-canvas)",
        elevated: "var(--ds-bg-elevated)",
        overlay: "var(--ds-bg-overlay)",
        hover: "var(--ds-bg-hover)",
        "input-bg": "var(--ds-bg-input)",

        "text-primary": "var(--ds-text-primary)",
        "text-secondary": "var(--ds-text-secondary)",
        "text-tertiary": "var(--ds-text-tertiary)",
        "text-disabled": "var(--ds-text-disabled)",

        "border-subtle": "var(--ds-border-subtle)",
        "border-default": "var(--ds-border-default)",
        "border-strong": "var(--ds-border-strong)",
        "border-focus": "var(--ds-border-focus)",

        brand: "var(--ds-brand)",
        "brand-hover": "var(--ds-brand-hover)",
        "brand-active": "var(--ds-brand-active)",
        "brand-soft": "var(--ds-brand-soft)",

        success: "var(--ds-success)",
        "success-soft": "var(--ds-success-soft)",
        warning: "var(--ds-warning)",
        "warning-soft": "var(--ds-warning-soft)",
        danger: "var(--ds-danger)",
        "danger-soft": "var(--ds-danger-soft)",

        reasoning: "var(--ds-reasoning-bg)",
        code: "var(--ds-code-bg)",
        tool: "var(--ds-tool-bg)",
      },
      fontFamily: {
        // 由 styles/index.css 的 body 规则覆盖,这里只做 utility 备用
        sans: [
          "Inter var",
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          "Segoe UI Variable Text",
          "Segoe UI",
          "PingFang SC",
          "Microsoft YaHei UI",
          "Noto Sans",
          "sans-serif",
        ],
        mono: [
          "JetBrains Mono",
          "SF Mono",
          "Cascadia Code",
          "Consolas",
          "Menlo",
          "monospace",
        ],
      },
      fontSize: {
        // 弃用 11px,改用 12px 起步
        xs: ["12px", { lineHeight: "16px" }],
        sm: ["13px", { lineHeight: "18px" }],
        base: ["14px", { lineHeight: "20px" }],
        // chat 正文专用 — 比 UI 大 1px,行高更松
        msg: ["15px", { lineHeight: "24px" }],
        lg: ["16px", { lineHeight: "24px" }],
        xl: ["20px", { lineHeight: "28px" }],
        "2xl": ["24px", { lineHeight: "32px" }],
      },
      borderRadius: {
        sm: "6px",
        md: "8px",
        lg: "12px",
        xl: "16px",
        "2xl": "20px",
      },
      keyframes: {
        "fade-in": {
          from: { opacity: "0" },
          to: { opacity: "1" },
        },
        "scale-in": {
          from: { opacity: "0", transform: "scale(0.96)" },
          to: { opacity: "1", transform: "scale(1)" },
        },
        // 浮层从下沿滑入（审批/问答面板）— 位移 + 淡入。
        "slide-up": {
          from: { opacity: "0", transform: "translateY(8px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "cursor-blink": {
          "0%, 50%": { opacity: "1" },
          "50.01%, 100%": { opacity: "0" },
        },
        "pulse-soft": {
          "0%, 100%": { opacity: "1", transform: "scale(1)" },
          "50%": { opacity: "0.55", transform: "scale(0.85)" },
        },
        // 文字扫光（"刷新光效"）—— 高光从左到右扫过进行时文案。
        // 用 background-position 配合 background-clip:text 实现。
        shimmer: {
          "0%": { backgroundPosition: "200% 0" },
          "100%": { backgroundPosition: "-200% 0" },
        },
        // reduced-motion 降级用：静态点的缓慢明暗，不位移不缩放。
        "breathe-dim": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.4" },
        },
      },
      animation: {
        "fade-in": "fade-in 150ms ease-out",
        "scale-in": "scale-in 150ms ease-out",
        "slide-up": "slide-up 150ms ease-out",
        "cursor-blink": "cursor-blink 1s steps(1) infinite",
        "pulse-soft": "pulse-soft 1.6s ease-in-out infinite",
        shimmer: "shimmer 2s linear infinite",
        "breathe-dim": "breathe-dim 2s ease-in-out infinite",
      },
    },
  },
  plugins: [],
};
