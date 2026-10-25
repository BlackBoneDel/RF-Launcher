import { useState } from 'react';
import { Play, Square, RefreshCw } from 'lucide-react';
import { launchInstance, killInstance } from '../lib/tauri';
import type { Instance } from '../types';

type Props = {
  instance: Instance;
  onStatusChange: () => void;
};

const btnBase =
  'inline-flex items-center justify-center rounded-md border shadow-sm focus:outline-none focus:ring-2 focus:ring-[var(--ring)] focus:ring-offset-0 transition-colors text-sm font-medium disabled:opacity-50 disabled:pointer-events-none';
const btnPrimary =
  `${btnBase} border-[var(--border)] bg-[var(--card)] text-[var(--fg)] hover:bg-[var(--accent-hover)] hover:text-white`;
const btnDanger =
  `${btnBase} border-[var(--danger)] text-[var(--danger)] hover:bg-[var(--danger)] hover:text-white`;

function ramLabel(value: number | null) {
  return value == null ? '—' : `${value} MB`;
}

export function InstanceCard({ instance, onStatusChange }: Props) {
  const [launching, setLaunching] = useState(false);
  const [killing, setKilling] = useState(false);
  const [message, setMessage] = useState<{ kind: 'ok' | 'error' | 'info'; text: string } | null>(null);

  const handleLaunch = async () => {
    setLaunching(true);
    setMessage(null);
    try {
      const res = await launchInstance(instance.id);
      if (res.ok) {
        setMessage({ kind: 'ok', text: `Iniciado (PID ${res.pid ?? '-'})` });
      } else {
        setMessage({ kind: 'error', text: res.error ?? 'Error al lanzar la instancia' });
      }
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
      if (res.terminated) {
        setMessage({ kind: 'ok', text: 'Instancia detenida' });
      } else if (res.ok) {
        setMessage({ kind: 'info', text: 'La instancia ya no estaba en ejecución' });
      } else {
        setMessage({ kind: 'error', text: res.error ?? 'Error al detener la instancia' });
      }
    } catch (err) {
      setMessage({ kind: 'error', text: String(err) });
    } finally {
      setKilling(false);
      onStatusChange();
    }
  };

  const kindStyles = {
    ok: 'border-[var(--success)] bg-[var(--success)]/10 text-[var(--success)]',
    error: 'border-[var(--danger)] bg-[var(--danger)]/10 text-[var(--danger)]',
    info: 'border-[var(--warn)] bg-[var(--warn)]/10 text-[var(--warn)]',
  };

  return (
    <div className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-4 flex flex-col md:flex-row md:items-center justify-between gap-4">
      <div className="flex flex-col gap-1 min-w-0">
        <h3 className="font-semibold text-[var(--fg)] truncate">{instance.name}</h3>
        <p className="text-xs text-[var(--muted)] truncate">
          Minecraft {instance.minecraft_version}
          {instance.fabric_loader ? ` · Fabric Loader {instance.fabric_loader}` : ''}
          {instance.template_origin ? ` · plantilla: {instance.template_origin}` : ''}
        </p>
        <div className="mt-1.5 flex items-center gap-2 text-xs text-[var(--muted)]">
          <span>RAM máx.:</span>
          <span className="font-mono text-[var(--fg)]">{ramLabel(instance.ram_max_mb)}</span>
        </div>
      </div>

      <div className="flex items-center gap-2 w-full md:w-auto justify-end">
        {message && (
          <div className={`px-3 py-1.5 rounded-md text-xs font-medium border ${kindStyles[message.kind]}`}>
            {message.text}
          </div>
        )}
        <button
          onClick={handleLaunch}
          disabled={launching || killing}
          className={btnPrimary + ' px-4'}
          title="Lanzar instancia"
        >
          {launching ? (
            <RefreshCw className="w-4 h-4 animate-spin" />
          ) : (
            <Play className="w-4 h-4" />
          )}
        </button>
        <button
          onClick={handleKill}
          disabled={killing || launching}
          className={btnDanger + ' px-4 sm:px-3'}
          title="Detener instancia"
        >
          <Square className="w-4 h-4" />
        </button>
      </div>
    </div>
  );
}
