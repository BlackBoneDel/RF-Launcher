import { useCallback, useEffect, useState } from 'react';
import { ArrowLeft } from 'lucide-react';
import { getSystemRam, getTheme, setGlobalConfig } from '../lib/tauri';
import type { ConfigKey, ConfigValue } from '../types';

type Props = {
  onBack: () => void;
};

const input =
  'block w-full rounded-md border border-[var(--border)] bg-[var(--card)] text-[var(--fg)] placeholder-[var(--muted)] shadow-sm focus:border-[var(--accent)] focus:ring-[var(--ring)] focus:outline-none sm:text-sm';
const btn =
  'inline-flex items-center justify-center rounded-md border border-[var(--border)] bg-[var(--card)] text-[var(--fg)] shadow-sm hover:bg-[var(--accent-hover)] hover:text-white focus:outline-none focus:ring-2 focus:ring-[var(--ring)] transition-colors text-sm font-medium px-3 py-2';
const themeBtn = (active: boolean) =>
  `flex-1 rounded-md py-2 text-xs font-medium transition-colors ${
    active
      ? 'bg-[var(--accent)] text-white shadow-sm'
      : 'hover:bg-[var(--accent-hover)]/10 text-[var(--fg)]'
  }`;

export function SettingsView({ onBack }: Props) {
  const [theme, setThemeState] = useState<'light' | 'dark'>('light');
  const [ramMin, setRamMin] = useState<string>('');
  const [ramMax, setRamMax] = useState<string>('');
  const [javaAuto, setJavaAuto] = useState(true);
  const [totalRam, setTotalRam] = useState<number | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [pendingForce, setPendingForce] = useState<{
    key: ConfigKey;
    value: ConfigValue;
    warning: string;
  } | null>(null);

  const load = useCallback(async () => {
    try {
      const t = await getTheme();
      setThemeState((t as string) === 'dark' ? 'dark' : 'light');
    } catch {}
    try {
      const sys = await getSystemRam();
      setTotalRam(sys.total_mb ?? null);
    } catch {}
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const intValue = (s: string): ConfigValue | null => {
    const n = Number(s);
    if (s.trim() === '' || !Number.isFinite(n)) return null;
    return { kind: 'Int', value: Math.floor(n) };
  };

  const saveGlobal = async (key: ConfigKey, value: ConfigValue, force = false) => {
    setMessage(null);
    setPendingForce(null);
    try {
      // El backend devuelve RamCheck: si ok=false y hay warning (>80% y <100%
      // de la RAM física) NO se guardó. Hay que poder confirmarlo con
      // force=true. El 100% o más se rechaza en duro y no llega aquí.
      const res = await setGlobalConfig(key, value, force);
      if (!res.ok && res.warning) {
        setPendingForce({ key, value, warning: res.warning });
        setMessage(res.warning);
      } else {
        setMessage('Guardado en la configuración global.');
      }
    } catch (err) {
      setMessage('Error al guardar: ' + String(err));
    }
  };

  return (
    <div className="max-w-2xl mx-auto">
      <div className="flex items-center gap-2 mb-6">
        <button
          onClick={onBack}
          className={btn}
          title="Volver"
        >
          <ArrowLeft className="w-4 h-4" />
        </button>
        <h2 className="text-lg font-semibold text-[var(--fg)]">Configuración</h2>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <div className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-4">
          <h3 className="text-sm font-semibold text-[var(--fg)] mb-2">Tema visual</h3>
          <p className="text-xs text-[var(--muted)] mb-3">
            La preferencia se guarda en la configuración global del launcher y se respeta entre reinicios.
          </p>
          <div className="flex gap-2">
            <button
              onClick={() => {
                setThemeState('light');
                void saveGlobal('Theme', { kind: 'String', value: 'light' });
              }}
              disabled={theme === 'light'}
              className={themeBtn(theme === 'light')}
              title="Tema claro"
            >
              Claro
            </button>
            <button
              onClick={() => {
                setThemeState('dark');
                void saveGlobal('Theme', { kind: 'String', value: 'dark' });
              }}
              disabled={theme === 'dark'}
              className={themeBtn(theme === 'dark')}
              title="Tema oscuro"
            >
              Oscuro
            </button>
          </div>
        </div>

        <div className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-4 col-span-full lg:col-span-2">
          <h3 className="text-sm font-semibold text-[var(--fg)] mb-3">RAM del sistema</h3>
          <div className="mb-3 flex items-center gap-3">
            <span className="text-[var(--muted)] text-sm">RAM física total detectada:</span>
            <span className="font-mono font-medium text-[var(--fg)]">
              {totalRam == null ? '—' : `${totalRam} MB`}
            </span>
          </div>
          <div className="space-y-3">
            <div>
              <label className="block text-sm font-medium text-[var(--fg)] mb-1">
                RAM mínima global (Xms)
              </label>
              <div className="flex gap-2">
                <input
                  className={input}
                  value={ramMin}
                  onChange={(e) => setRamMin(e.target.value)}
                  placeholder="ej. 1024"
                />
                <button
                  onClick={() => {
                    const v = intValue(ramMin);
                    if (v) void saveGlobal('RamMinMb', v);
                  }}
                  className={btn}
                >
                  Fijar
                </button>
              </div>
              <p className="mt-1 text-xs text-[var(--muted)]">
                Valor por defecto del sistema: 1024 MB.
              </p>
            </div>
            <div>
              <label className="block text-sm font-medium text-[var(--fg)] mb-1">
                RAM máxima global (Xmx)
              </label>
              <div className="flex gap-2">
                <input
                  className={input}
                  value={ramMax}
                  onChange={(e) => setRamMax(e.target.value)}
                  placeholder="ej. 2048"
                />
                <button
                  onClick={() => {
                    const v = intValue(ramMax);
                    if (v) void saveGlobal('RamMaxMb', v);
                  }}
                  className={btn}
                >
                  Fijar
                </button>
              </div>
              <p className="mt-1 text-xs text-[var(--muted)]">
                Valor por defecto del sistema: 2048 MB. 100 % o más de la RAM física se rechaza en duro.
              </p>
            </div>
            <div className="flex items-start gap-3">
              <input
                id="java-auto"
                type="checkbox"
                checked={javaAuto}
                onChange={(e) => {
                  setJavaAuto(e.target.checked);
                  void saveGlobal('JavaAutoDownload', { kind: 'Bool', value: e.target.checked });
                }}
                className="rounded border-[var(--border)] bg-[var(--card)] text-[var(--accent)] focus:ring-[var(--ring)]"
              />
              <label htmlFor="java-auto" className="text-sm text-[var(--fg)]">
                Descargar Java automáticamente desde Adoptium si falta
              </label>
            </div>
          </div>
        </div>
      </div>

      {message && (
        <div
          className={`mt-4 p-3 rounded-md border text-sm ${
            pendingForce
              ? 'border-[var(--warn)] bg-[var(--warn)]/10 text-[var(--warn)]'
              : 'border-[var(--success)] bg-[var(--success)]/10 text-[var(--success)]'
          }`}
        >
          {message}
          {pendingForce && (
            <button
              className={`${btn} mt-2 block`}
              onClick={() =>
                pendingForce &&
                void saveGlobal(pendingForce.key, pendingForce.value, true)
              }
            >
              Confirmar de todos modos (force)
            </button>
          )}
        </div>
      )}
    </div>
  );
}
