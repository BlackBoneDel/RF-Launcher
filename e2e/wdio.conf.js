import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const __dirname = fileURLToPath(new URL('.', import.meta.url));
const repoRoot = path.resolve(__dirname, '..');
const homeDir = path.resolve(repoRoot, '.e2e-home');
const dataRoot = path.resolve(homeDir, '.local', 'share');
const pidFile = path.resolve(repoRoot, '.e2e-java.pid');
const fixturePort = Number(process.env.RF_E2E_FIXTURE_PORT || 47831);
const adoptiumBase = `http://127.0.0.1:${fixturePort}/v3`;
const templateDir = path.resolve(__dirname, 'fixtures', 'jdk-template');
const archivePath = path.resolve(repoRoot, '.e2e-jdk.tar.gz');
const appBinary = path.resolve(repoRoot, 'src-tauri', 'target', 'release', 'rf-launcher');

let fixtureServer;
let tauriDriver;
let archiveSha256 = '';
let archiveBytes = Buffer.alloc(0);

function buildJdkFixture() {
  fs.rmSync(archivePath, { force: true });
  const javaBin = path.join(templateDir, 'jdk-21', 'bin', 'java');
  fs.chmodSync(javaBin, 0o755);
  const result = spawnSync('tar', ['-czf', archivePath, '-C', templateDir, 'jdk-21'], {
    stdio: 'inherit',
  });
  if (result.status !== 0) throw new Error('No se pudo crear el tar.gz del JDK fixture');
  archiveBytes = fs.readFileSync(archivePath);
  archiveSha256 = crypto.createHash('sha256').update(archiveBytes).digest('hex');
}

function serveArchive(res) {
  res.writeHead(200, {
    'Content-Type': 'application/gzip',
    'Content-Length': archiveBytes.length,
    'Cache-Control': 'no-store',
  });
  // Stream real en varios chunks: permite que Rust emita progreso con
  // Content-Length y que el test observe un porcentaje >0, no un evento único.
  const chunkSize = Math.max(1, Math.ceil(archiveBytes.length / 12));
  let offset = 0;
  const writeNext = () => {
    if (offset >= archiveBytes.length) {
      res.end();
      return;
    }
    const end = Math.min(offset + chunkSize, archiveBytes.length);
    res.write(archiveBytes.subarray(offset, end));
    offset = end;
    setTimeout(writeNext, 40);
  };
  writeNext();
}

function createFixtureServer() {
  return http.createServer((req, res) => {
    const url = req.url || '';
    if (url.startsWith('/v3/assets/feature_releases/21/ga')) {
      const payload = [
        {
          binaries: [
            {
              architecture: 'x64',
              os: 'linux',
              image_type: 'jdk',
              package: {
                link: `${adoptiumBase}/download/jdk.tar.gz`,
                checksum: archiveSha256,
              },
            },
          ],
        },
      ];
      const body = JSON.stringify(payload);
      res.writeHead(200, { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(body) });
      res.end(body);
      return;
    }
    if (url.startsWith('/v3/download/jdk.tar.gz')) {
      serveArchive(res);
      return;
    }
    res.writeHead(404, { 'Content-Type': 'text/plain' });
    res.end('not found');
  });
}

async function startFixtureServer() {
  buildJdkFixture();
  fixtureServer = createFixtureServer();
  await new Promise((resolve, reject) => {
    fixtureServer.once('error', reject);
    fixtureServer.listen(fixturePort, '127.0.0.1', resolve);
  });
  console.log(`E2E_FIXTURE_SERVER=http://127.0.0.1:${fixturePort} jdk_sha256=${archiveSha256}`);
}

async function waitForHttp(url, timeoutMs) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        console.log(`E2E_DRIVER_READY=${url}`);
        return;
      }
    } catch {
      // El driver aún no escucha.
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  throw new Error(`El driver WebDriver no respondió /status en ${timeoutMs}ms`);
}

function closeFixtureServer() {
  if (fixtureServer) {
    fixtureServer.close();
    fixtureServer = undefined;
  }
}

function closeTauriDriver() {
  if (tauriDriver && !tauriDriver.killed) {
    tauriDriver.kill();
    tauriDriver = undefined;
  }
}

function onShutdown(fn) {
  const cleanup = () => {
    try {
      fn();
    } finally {
      // WDIO gestiona su propio ciclo; no forzamos process.exit aquí para no
      // cortar la escritura del reporte.
    }
  };
  process.on('exit', cleanup);
  process.on('SIGINT', cleanup);
  process.on('SIGTERM', cleanup);
  process.on('SIGHUP', cleanup);
}

export const config = {
  hostname: '127.0.0.1',
  port: 4444,
  specs: ['./specs/**/*.js'],
  maxInstances: 1,
  capabilities: [
    {
      maxInstances: 1,
      'tauri:options': {
        application: appBinary,
      },
    },
  ],
  reporters: ['spec'],
  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 300000,
  },
  waitforTimeout: 30000,
  connectionRetryCount: 3,
  connectionRetryTimeout: 120000,
  logLevel: 'warn',

  onPrepare: async () => {
    fs.rmSync(homeDir, { recursive: true, force: true });
    fs.rmSync(pidFile, { force: true });
    fs.mkdirSync(dataRoot, { recursive: true });

    // Variables heredadas por tauri-driver -> aplicación Tauri real.
    process.env.HOME = homeDir;
    process.env.XDG_DATA_HOME = dataRoot;
    process.env.GDK_BACKEND = 'x11';
    process.env.LIBGL_ALWAYS_SOFTWARE = '1';
    process.env.WEBKIT_DISABLE_DMABUF_RENDERER = '1';
    process.env.WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS = '1';
    process.env.RF_LAUNCHER_E2E = '1';
    process.env.RF_LAUNCHER_E2E_ADOPTIUM_BASE = adoptiumBase;
    process.env.RF_LAUNCHER_E2E_MINECRAFT_FIXTURE = '1';
    process.env.RF_LAUNCHER_E2E_PID_FILE = pidFile;
    process.env.RF_LAUNCHER_E2E_DISABLE_SYSTEM_JAVA = '1';

    await startFixtureServer();
    console.log(`E2E_APP_BINARY=${appBinary}`);
    console.log(`E2E_DATA_ROOT=${dataRoot}`);
  },

  beforeSession: async () => {
    const driver = process.env.TAURI_DRIVER || path.resolve(os.homedir(), '.cargo', 'bin', 'tauri-driver');
    const nativeDriver = process.env.WEBKIT_WEBDRIVER || '/usr/bin/WebKitWebDriver';
    const args = [
      '--port', '4444',
      '--native-port', '4445',
      '--native-driver', nativeDriver,
    ];
    console.log(`E2E_TAURI_DRIVER=${driver} ${args.join(' ')}`);
    tauriDriver = spawn(driver, args, {
      stdio: ['ignore', 'pipe', 'pipe'],
      env: process.env,
    });
    tauriDriver.stdout?.on('data', (d) => process.stdout.write(`[tauri-driver] ${d}`));
    tauriDriver.stderr?.on('data', (d) => process.stderr.write(`[tauri-driver] ${d}`));
    tauriDriver.on('error', (error) => {
      console.error('tauri-driver error:', error);
    });
    tauriDriver.on('exit', (code) => {
      if (code !== 0 && code !== null) {
        console.error(`tauri-driver exited with code ${code}`);
      }
    });

    // Espera explícita a que el proxy y el driver nativo estén listos.
    await waitForHttp('http://127.0.0.1:4444/status', 30000);
  },

  afterSession: () => {
    closeTauriDriver();
    closeFixtureServer();
  },
};

onShutdown(() => {
  closeTauriDriver();
  closeFixtureServer();
});
