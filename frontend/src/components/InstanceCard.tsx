import { useCallback, useEffect, useState } from 'react';
import { Play, Square, RefreshCw, Settings2, ChevronDown, ChevronUp } from 'lucide-react';
import { launchInstance, killInstance, resolveConfig, setInstanceConfig } from '../lib/tauri';
import type { ConfigKey, ConfigValue, Instance } from '../types';

type Props = {
  instance: Instance;
  onStatusChange: () => void;
};

const btnBase =
  'inline-flex items-center justify-center rounded-md border shadow-sm focus:outline-none focus:ring-2 focus:ring-[var(--ring)] transition-colors text-sm font-medium disabled:opacity-50 disabled:pointer-events-none';
const btnPrimary = `${btnBase} border-[var(--border)] bg-[var(--card)] text-[var(--fg)] hover:bg-[var(--accent-hover)] hover:text-white`;
const btnDanger = `${btnBase} border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--danger)] hover:text-white`;
const btnSmall = `${btnBase} border-[var(--border)] bg-[var(--card)] text-[var(--fg)] hover:bg-[var(--accent-hover)] hover:text-white px-2.5 py-1.5 text-xs`;
const input =
  'block w-full rounded-md border border-[var(--border)] bg-[var(--card)] text-[var(--fg)] placeholder-[var(--muted)] shadow-sm focus:border-[var(--accent)] focus:ring-[var(--ring)] focus:outline-none sm:text-sm px-2 py-1.5';

function ramLabel(value: number | null) {
  return value == null ? 'hereda global' : `${value} MB`;
}

function intOf(v: ConfigValue | null): number | null {
  if (!v || v.kind !== 'Int') return null;
  const n = Number(v.value);
  return Number.isFinite(n) ? n : null;
}

export function InstanceCard({ instance, onStatusChange }: Props) {
  const [launching, setLaunching] = useState(false);
  const [killing, setKilling] = useState(false);
  const [message, setMessage] = useState<{ kind: 'ok' | 'error' | 'info'; text: string } | null>(null);

  const [showConfig, setShowConfig] = useState(false);
  const [ramMin, setRamMin] = useState('');
  const [ramMax, setRamMax] = useState('');
  const [effective, setEffective] = useState<{ min: number | null; max: number | null }>({
    min: null,
    max: null,
  });
  const [warning, setWarning] = useState<string | null>(null);
  const [pendingForce, setPendingForce] = useState<{ key: ConfigKey; value: ConfigValue } | null>(null);
  const [saving, setSaving] = useState(false);

  const loadEffective = useCallback(async () => {
    try {
      const [min, max] = await Promise.all([
        resolveConfig(instance.id, 'RamMinMb'),
        resolveConfig(instance.id, 'RamMaxMb'),
      ]);
      setEffective({ min: intOf(min), max: intOf(max) });
    } catch {
      // Sin backend (vista previa en navegador): se muestran los overrides locales.
      setEffective({ min: instance.ramMinMb, max: instance.ramMaxMb });
    }
  }, [instance.id, instance.ramMinMb, instance.ramMaxMb]);

  useEffect(() => {
    if (showConfig) void loadEffective();
  }, [showConfig, loadEffective]);

  const handleLaunch = async () => {
    setLaunching(true);
    setMessage(null);
    try {
      const res = await launchInstance(instance.id);
      setMessage(
        res.ok
          ? { kind: 'ok', text: `Iniciado (PID ${res.pid ?? '-'})` }
          : { kind: 'error', text: res.error ?? 'Error al lanzar la instancia' },
      );
    } catch (err) {
      setMessage({ kind: 'error', text: String(err) });
    } finally {
      setLaunching(false);
      onStatusChange();
    }
  };

  const handleKill = async () => {
    setKilling(true);
    setMessage(null);
    try {
      const res = await killInstance(instance.id);
      if (res.terminated) setMessage({ kind: 'ok', text: 'Instancia detenida (PID verificado)' });
      else if (res.ok) setMessage({ kind: 'info', text: 'La instancia ya no estaba en ejecución' });
      else setMessage({ kind: 'error', text: res.error ?? 'Error al detener la instancia' });
    } catch (err) {
      setMessage({ kind: 'error', text: String(err) });
    } finally {
      setKilling(false);
      onStatusChange();
    }
  };

  const saveRam = async (key: ConfigKey, raw: string, force = false) => {
    const n = Number(raw);
    if (raw.trim() === '' || !Number.isFinite(n) || n <= 0) {
      setMessage({ kind: 'error', text: 'Introduce un valor de RAM en MB mayor que 0.' });
      return;
    }
    setSaving(true);
    setWarning(null);
    setPendingForce(null);
    try {
      const value: ConfigValue = { kind: 'Int', value: Math.floor(n) };
      const res = await setInstanceConfig(instance.id, key, value, force);
      if (!res.ok && res.warning) {
        setWarning(res.warning);
        setPendingForce({ key, value });
      } else {
        setMessage({ kind: 'ok', text: 'Override de RAM guardado para esta instancia.' });
        await loadEffective();
        onStatusChange();
      }
    } catch (err) {
      setMessage({ kind: 'error', text: String(err) });
    } finally {
      setSaving(false);
    }
  };

  const kindStyles = {
    ok: 'border-[var(--success)] bg-[var(--success)]/10 text-[var(--success)]',
    error: 'border-[var(--danger)] bg-[var(--danger)]/10 text-[var(--danger)]',
    info: 'border-[var(--warn)] bg-[var(--warn)]/10 text-[var(--warn)]',
  };

  return (
    <div className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-4 flex flex-col gap-3">
      <div className="flex flex-col gap-1 min-w-0">
        <h3 className="font-semibold text-[var(--fg)] truncate">{instance.name}</h3>
        <p className="text-xs text-[var(--muted)] truncate">
          Minecraft {instance.minecraftVersion}
          {instance.fabricLoader ? ` · Fabric Loader ${instance.fabricLoader}` : ' · vanilla'}
        </p>
        <div className="mt-1.5 flex items-center gap-2 text-xs text-[var(--muted)]">
          <span>RAM máx.:</span>
          <span className="font-mono text-[var(--fg)]">{ramLabel(instance.ramMaxMb)}</span>
        </div>
      </div>

      <div className="flex items-center gap-2 w-full justify-end flex-wrap">
        {message && (
          <div className={`px-3 py-1.5 rounded-md text-xs font-medium border ${kindStyles[message.kind]}`}>
            {message.text}
          </div>
        )}
        <button
          onClick={() => setShowConfig((v) => !v)}
          className={btnSmall}
          title="Configuración de esta instancia"
        >
          <Settings2 className="w-3.5 h-3.5 mr-1" />
          RAM
          {showConfig ? <ChevronUp className="w-3.5 h-3.5 ml-1" /> : <ChevronDown className="w-3.5 h-3.5 ml-1" />}
        </button>
        <button onClick={handleLaunch} disabled={launching || killing} className={btnPrimary + ' px-4'} title="Lanzar instancia">
          {launching ? <RefreshCw className="w-4 h-4 animate-spin" /> : <Play className="w-4 h-4" />}
        </button>
        <button onClick={handleKill} disabled={killing || launching} className={btnDanger + ' px-4 sm:px-3'} title="Detener instancia">
          <Square className="w-4 h-4" />
        </button>
      </div>

      {showConfig && (
        <div className="border-t border-[var(--border)] pt-3 mt-1 space-y-3">
          <p className="text-xs text-[var(--muted)]">
            Valores efectivos resueltos por el motor jerárquico: Xms{' '}
            <span className="font-mono text-[var(--fg)]">{effective.min ?? '—'} MB</span> · Xmx{' '}
            <span className="font-mono text-[var(--fg)]">{effective.max ?? '—'} MB</span>.
            Si no se fija un override, se hereda el valor global.
          </p>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs font-medium text-[var(--fg)] mb-1">RAM mínima (Xms, MB)</label>
              <div className="flex gap-1.5">
                <input
                  className={input}
                  value={ramMin}
                  onChange={(e) => setRamMin(e.target.value)}
                  placeholder={String(effective.min ?? 1024)}
                  inputMode="numeric"
                />
                <button className={btnSmall} disabled={saving} onClick={() => saveRam('RamMinMb', ramMin)}>
                  Fijar
                </button>
              </div>
            </div>
            <div>
              <label className="block text-xs font-medium text-[var(--fg)] mb-1">RAM máxima (Xmx, MB)</label>
              <div className="flex gap-1.5">
                <input
                  className={input}
                  value={ramMax}
                  onChange={(e) => setRamMax(e.target.value)}
                  placeholder={String(effective.max ?? 2048)}
                  inputMode="numeric"
                />
                <button className={btnSmall} disabled={saving} onClick={() => saveRam('RamMaxMb', ramMax)}>
                  Fijar
                </button>
              </div>
            </div>
          </div>
          {warning && pendingForce && (
            <div className="p-2.5 rounded-md border border-[var(--warn)] bg-[var(--warn)]/10 text-xs text-[var(--warn)] space-y-2">
              <p>{warning}</p>
              <button
                className={btnSmall}
                disabled={saving}
                onClick={() => saveRam(pendingForce.key, String(pendingForce.value.value), true)}
              >
                Confirmar de todos modos (force)
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
