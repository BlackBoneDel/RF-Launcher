import { Plus } from 'lucide-react';
import { InstanceCard } from './InstanceCard';
import type { Instance } from '../types';

type Props = {
  instances: Instance[];
  onCreate: () => void;
  onRefresh: () => void;
};

export function InstanceList({ instances, onCreate, onRefresh }: Props) {
  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold text-[var(--fg)]">Instancias</h2>
        <button
          data-testid="new-instance"
          onClick={onCreate}
          className="inline-flex items-center justify-center rounded-md border border-[var(--border)] bg-[var(--card)] text-[var(--fg)] shadow-sm hover:bg-[var(--accent-hover)] hover:text-white focus:outline-none focus:ring-2 focus:ring-[var(--ring)] transition-colors text-sm font-medium px-3 py-2"
        >
          <Plus className="w-4 h-4 mr-2" />
          Nueva instancia
        </button>
      </div>
      {instances.length === 0 ? (
        <div
          data-testid="empty-instances"
          className="rounded-xl border border-[var(--border)] bg-[var(--card)] shadow-sm p-8 text-center text-[var(--muted)]"
        >
          No hay instancias creadas. Crea tu primera instancia desde el botón de arriba.
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {instances.map((inst) => (
            <InstanceCard key={inst.id} instance={inst} onStatusChange={onRefresh} />
          ))}
        </div>
      )}
    </div>
  );
}
