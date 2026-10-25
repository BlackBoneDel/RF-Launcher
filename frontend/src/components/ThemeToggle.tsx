import { Moon, Sun } from 'lucide-react';
import { setTheme } from '../lib/tauri';

type Props = { theme: 'light' | 'dark'; onChange: (theme: 'light' | 'dark') => void };

export function ThemeToggle({ theme, onChange }: Props) {
  const pick = (t: 'light' | 'dark') => {
    onChange(t);
    void setTheme(t).catch((err) => {
      console.warn('No se pudo guardar el tema en la configuración global:', err);
    });
  };

  const base =
    'rounded-md px-3 py-1.5 text-xs font-medium transition-colors';
  const active = `${base} bg-[var(--accent)] text-white shadow-sm`;
  const inactive = `${base} hover:bg-[var(--accent-hover)]/10 text-[var(--fg)]`;

  return (
    <div className="flex items-center gap-2 border border-[var(--border)] rounded-md p-1 bg-[var(--card)] text-[var(--fg)]">
      <button
        onClick={() => pick('light')}
        disabled={theme === 'light'}
        className={theme === 'light' ? active : inactive}
        title="Tema claro"
      >
        <Sun className="w-3 h-3" />
        <span className="ml-1.5 hidden md:inline">Claro</span>
      </button>
      <button
        onClick={() => pick('dark')}
        disabled={theme === 'dark'}
        className={theme === 'dark' ? active : inactive}
        title="Tema oscuro"
      >
        <Moon className="w-3 h-3" />
        <span className="ml-1.5 hidden md:inline">Oscuro</span>
      </button>
    </div>
  );
}
