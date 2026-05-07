/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}'],
  theme: {
    extend: {
      colors: {
        bg: 'var(--bg)',
        text: 'var(--text)',
        surface: 'var(--surface)',
        'surface-1': 'var(--surface-1)',
        'surface-2': 'var(--surface-2)',
        'surface-3': 'var(--surface-3)',
        border: 'var(--border)',
        muted: 'var(--muted)',
        'hover-bg': 'var(--hover-bg)',
        accent: 'var(--accent)',
        'accent-hover': 'var(--accent-hover)',
        success: 'var(--success)',
        danger: 'var(--danger)',
        warning: 'var(--warning)',
        info: 'var(--info)',
      },
      fontFamily: {
        heading: ['Playfair Display', 'Georgia', 'serif'],
        body: ['Inter', '-apple-system', 'sans-serif'],
        mono: ['JetBrains Mono', 'Fira Code', 'monospace'],
      },
      maxWidth: {
        content: '960px',
        prose: '720px',
        narrow: '600px',
      },
      spacing: {
        18: '4.5rem',
        30: '7.5rem',
        32: '8rem',
      },
      borderRadius: {
        DEFAULT: '0',
        card: '8px',
        modal: '12px',
        badge: '4px',
        tooltip: '6px',
      },
      boxShadow: {
        DEFAULT: 'none',
        sm: 'var(--shadow-sm)',
        md: 'var(--shadow-md)',
        lg: 'var(--shadow-lg)',
        glow: 'var(--shadow-glow)',
      },
      transitionTimingFunction: {
        default: 'var(--ease-default)',
        out: 'var(--ease-out)',
        spring: 'var(--ease-spring)',
      },
    },
  },
  plugins: [],
};
