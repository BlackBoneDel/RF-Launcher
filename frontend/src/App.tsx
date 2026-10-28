import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { ThemeToggle } from './components/ThemeToggle';
import { InstanceList } from './components/InstanceList';
import { CreateInstanceForm } from './components/CreateInstanceForm';
import { SettingsView } from './components/SettingsView';
import type { Instance, JavaProgressEvent } from './types';

type View = 'instances' | 'create' | 'settings';

export default function App() {
  const [view, setView] = useState<View>('instances');
  const [instances, setInstances] = useState<Instance[]>([]);
  const [theme, setTheme] = useState<'light' | 'dark'>('light');
  const [ready, setReady] = useState(false);
  const [javaProgress, setJavaProgress] = useState<JavaProgressEvent | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const t = await invoke<string>('get_theme');
        setTheme((t as string) === 'dark' ? 'dark' : 'light');
      } catch {
        // Tauri backend no disponible (vista previa en navegador sin Rust): usa localStorage como respaldo.
        const stored = localStorage.getItem('rf:theme') as 'light' | 'dark' | null;
        if (stored) setTheme(stored);
      }
      setReady(true);
    })();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<JavaProgressEvent>('java-download-progress', (event) => {
      setJavaProgress(event.payload);
      if (event.payload.stage === 'ready') {
        window.setTimeout(() => setJavaProgress(null), 1500);
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<Instance[]>('list_instances');
      setInstances(list);
    } catch {
      // Sin backend: lista vacía.
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const applyTheme = (t: string) => {
    const root = document.documentElement;
    root.classList.remove('light', 'dark');
    root.classList.add(t);
    localStorage.setItem('rf:theme', t);
  };

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  const handleThemeChange = async (t: 'light' | 'dark') => {
    setTheme(t);
    try {
      await invoke('set_theme', { theme: t });
    } catch {
      // Se persistió en localStorage por applyTheme; el backend lo guardará al reconectar.
    }
  };

  const handleCreate = async (values: {
    name: string;
    minecraftVersion: string;
    fabricLoader: string | null;
  }) => {
    try {
      // Las claves son camelCase: Tauri 2 las mapea a los parámetros
      // snake_case del comando Rust `create_instance`.
      await invoke('create_instance', values);
      setView('instances');
      await refresh();
    } catch (err) {
      alert('No se pudo crear la instancia: ' + String(err));
    }
  };

  if (!ready) {
    return (
      <div className="flex min-h-screen items-center justify-center text-[var(--muted)]">
        Cargando RF Launcher…
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-[var(--bg)] text-[var(--fg)] transition-colors duration-200">
      <header className="border-b border-[var(--border)] bg-[var(--card)]/60 backdrop-blur sticky top-0 z-10">
        <div className="flex items-center justify-between px-6 py-3">
          <div className="flex items-center gap-3">
            <span className="font-bold text-[var(--fg)] tracking-tight">RF Launcher</span>
            <span className="text-xs text-[var(--muted)]">v0.1.0 · Fase 3-A.2 · sub-ronda 1</span>
          </div>
          <div className="flex items-center gap-3">
            <ThemeToggle theme={theme} onChange={handleThemeChange} />
            <button
              onClick={() => setView('settings')}
              className="btn text-sm"
            >
              Configuración
            </button>
          </div>
        </div>
      </header>

      {javaProgress && (
        <div
          data-testid="java-progress"
          className="mx-6 mt-4 rounded-md border border-[var(--border)] bg-[var(--card)] p-3 text-sm"
        >
          <div className="flex items-center justify-between gap-3">
            <span>{javaProgress.message || `Descargando Java ${javaProgress.version}…`}</span>
            <span className="font-mono text-xs">
              {javaProgress.percent != null
                ? `${javaProgress.percent}%`
                : javaProgress.totalBytes != null
                  ? `${javaProgress.downloadedBytes}/${javaProgress.totalBytes} bytes`
                  : javaProgress.stage}
            </span>
          </div>
          {javaProgress.percent != null && (
            <div className="mt-2 h-2 rounded bg-[var(--border)] overflow-hidden">
              <div
                className="h-full bg-[var(--accent)]"
                style={{ width: `${javaProgress.percent}%` }}
              />
            </div>
          )}
        </div>
      )}

      <main className="p-6">
        {view === 'instances' && (
          <InstanceList instances={instances} onCreate={() => setView('create')} onRefresh={refresh} />
        )}
        {view === 'create' && (
          <CreateInstanceForm
            onSubmit={handleCreate}
            onBack={() => setView('instances')}
          />
        )}
        {view === 'settings' && <SettingsView onBack={() => setView('instances')} />}
      </main>

      <footer className="border-t border-[var(--border)] mt-auto px-6 py-3 text-xs text-[var(--muted)]">
        Núcleo de gestión de instancias — Rust + Tauri · Sin mods, sin cuentas, sin RF Client hardcodeado.
      </footer>
    </div>
  );
}
