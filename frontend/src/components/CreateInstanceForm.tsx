import { useState } from 'react';
import { ArrowLeft } from 'lucide-react';

type Props = {
  onSubmit: (values: {
    name: string;
    minecraftVersion: string;
    fabricLoader: string | null;
  }) => void;
  onBack: () => void;
};

const input =
  'block w-full rounded-md border border-[var(--border)] bg-[var(--card)] text-[var(--fg)] placeholder-[var(--muted)] shadow-sm focus:border-[var(--accent)] focus:ring-[var(--ring)] focus:outline-none sm:text-sm';
const btnBase =
  'inline-flex items-center justify-center rounded-md border shadow-sm focus:outline-none focus:ring-2 focus:ring-[var(--ring)] transition-colors text-sm font-medium disabled:opacity-50 disabled:pointer-events-none';
const btn =
  `${btnBase} border-[var(--border)] bg-[var(--card)] text-[var(--fg)] hover:bg-[var(--accent-hover)] hover:text-white`;
const btnPrimary = `${btnBase} border-[var(--border)] bg-[var(--card)] text-[var(--fg)] hover:bg-[var(--accent-hover)] hover:text-white`;

export function CreateInstanceForm({ onSubmit, onBack }: Props) {
  const [name, setName] = useState('');
  const [minecraftVersion, setMinecraftVersion] = useState('1.20.1');
  const [useFabric, setUseFabric] = useState(true);
  const [fabricVersion, setFabricVersion] = useState('0.14.9');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setSubmitting(true);
    try {
      await onSubmit({
        name: name.trim(),
        minecraftVersion: minecraftVersion.trim(),
        fabricLoader: useFabric ? fabricVersion.trim() || null : null,
      });
    } catch (err) {
      setError(String(err));
    } finally {
      setSubmitting(false);
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
        <h2 className="text-lg font-semibold text-[var(--fg)]">Nueva instancia</h2>
      </div>

      {error && (
        <div className="mb-4 p-3 rounded-md border border-[var(--danger)] bg-[var(--danger)]/10 text-[var(--danger)] text-sm">
          {error}
        </div>
      )}

      <form onSubmit={handleSubmit} className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-6 space-y-4">
        <div>
          <label className="block text-sm font-medium text-[var(--fg)] mb-1">Nombre de la instancia</label>
          <input
            data-testid="instance-name"
            className={input}
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="Ej. Supervivencia 1.20.1"
            required
          />
        </div>

        <div>
          <label className="block text-sm font-medium text-[var(--fg)] mb-1">Versión de Minecraft</label>
          <input
            data-testid="minecraft-version"
            className={input}
            value={minecraftVersion}
            onChange={(e) => setMinecraftVersion(e.target.value)}
            placeholder="Ej. 1.20.1"
            required
          />
          <p className="mt-1 text-xs text-[var(--muted)]">
            Cualquier versión de release de Minecraft. Las versiones antiguas pueden requerir Java 8 o 17.
          </p>
        </div>

        <div className="flex items-center gap-3">
          <input
            id="fabric"
            data-testid="use-fabric"
            type="checkbox"
            className="rounded border-[var(--border)] bg-[var(--card)] text-[var(--accent)] focus:ring-[var(--ring)]"
            checked={useFabric}
            onChange={(e) => setUseFabric(e.target.checked)}
          />
          <label htmlFor="fabric" className="text-sm text-[var(--fg)]">
            Instalar Fabric Loader
          </label>
          <span className="text-[var(--muted)] text-xs mx-2">·</span>
          <input
            data-testid="fabric-version"
            className={`${input} w-32`}
            value={fabricVersion}
            onChange={(e) => setFabricVersion(e.target.value)}
            disabled={!useFabric}
            placeholder="0.14.9"
          />
        </div>

        <div className="flex items-center justify-between pt-2">
          <button type="button" onClick={onBack} className={btn}>
            Cancelar
          </button>
          <button data-testid="create-instance-submit" type="submit" disabled={submitting} className={btnPrimary}>
            {submitting ? 'Creando…' : 'Crear instancia'}
          </button>
        </div>
      </form>
    </div>
  );
}
