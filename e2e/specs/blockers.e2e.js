import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const dataRoot = process.env.XDG_DATA_HOME;
const appData = path.join(dataRoot, 'io.github.blackbonedel.rflauncher');
const instancesDir = path.join(appData, 'instances');
const pidFile = process.env.RF_LAUNCHER_E2E_PID_FILE;

function instanceDir(id) {
  return path.join(instancesDir, id);
}

function readInstance(id) {
  return JSON.parse(fs.readFileSync(path.join(instanceDir(id), 'instance.json'), 'utf8'));
}

function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    if (err && err.code === 'ESRCH') return false;
    throw err;
  }
}

async function waitForText(testid, expected, timeout = 30000) {
  await browser.waitUntil(
    async () => (await browser.$(`[data-testid="${testid}"]`).getText()).includes(expected),
    { timeout, timeoutMsg: `No apareció "${expected}" en ${testid}` },
  );
}

describe('Fase 3-A.2 sub-ronda 1 — bloqueantes UI → IPC → backend', () => {
  let instanceId;

  before(async () => {
    assert.ok(dataRoot, 'XDG_DATA_HOME no fue configurado por wdio.conf.js');
    assert.ok(pidFile, 'RF_LAUNCHER_E2E_PID_FILE no fue configurado');
    await browser.waitUntil(async () => (await browser.$('body').getText()).includes('Instancias'), {
      timeout: 30000,
    });
  });

  it('crea una instancia real desde la UI', async () => {
    await browser.$('[data-testid="new-instance"]').click();
    await browser.$('[data-testid="instance-name"]').setValue('E2E Base');
    await browser.$('[data-testid="minecraft-version"]').setValue('1.21');
    const useFabric = await browser.$('[data-testid="use-fabric"]');
    if (await useFabric.isSelected()) await useFabric.click();
    await browser.$('[data-testid="create-instance-submit"]').click();

    await browser.waitUntil(async () => (await browser.$$('[data-testid^="instance-card-"]')).length === 1, {
      timeout: 30000,
    });
    const dirs = fs.readdirSync(instancesDir).filter((name) => fs.statSync(path.join(instancesDir, name)).isDirectory());
    assert.equal(dirs.length, 1, 'Debe existir exactamente un directorio de instancia');
    instanceId = dirs[0];
    const json = readInstance(instanceId);
    assert.equal(json.name, 'E2E Base');
    assert.equal(json.minecraftVersion, '1.21');
    assert.equal(json.fabricLoader, null);
  });

  it('B — editar parámetros de una instancia existente persiste y refresca la UI', async () => {
    await browser.$(`[data-testid="edit-instance-${instanceId}"]`).click();
    await browser.$('[data-testid="edit-name"]').setValue('E2E Editada');
    await browser.$('[data-testid="edit-minecraft-version"]').setValue('1.21.1');
    await browser.$('[data-testid="edit-save"]').click();

    await waitForText(`instance-name-${instanceId}`, 'E2E Editada');
    await waitForText(`instance-version-${instanceId}`, '1.21.1');
    const json = readInstance(instanceId);
    assert.equal(json.name, 'E2E Editada');
    assert.equal(json.minecraftVersion, '1.21.1');
  });

  it('D — override jerárquico se refleja y se puede limpiar', async () => {
    await browser.$(`[data-testid="ram-config-${instanceId}"]`).click();
    await browser.$(`[data-testid="ram-max-input-${instanceId}"]`).setValue('4096');
    await browser.$(`[data-testid="ram-max-save-${instanceId}"]`).click();

    await waitForText(`ram-label-${instanceId}`, '4096 MB');
    let json = readInstance(instanceId);
    assert.equal(json.ramMaxMb, 4096, 'ramMaxMb debe sincronizarse con el override');
    assert.ok(json.configOverrides.RamMaxMb, 'configOverrides debe conservar el override');

    await browser.$(`[data-testid="ram-max-clear-${instanceId}"]`).click();
    await waitForText(`ram-label-${instanceId}`, 'hereda global');
    json = readInstance(instanceId);
    assert.equal(json.ramMaxMb, null, 'ramMaxMb debe volver a null al limpiar');
    assert.ok(!json.configOverrides.RamMaxMb, 'El override debe eliminarse');
  });

  it('E/F — Java con progreso real, lanzamiento real y G — kill real por PID', async () => {
    await browser.$(`[data-testid="launch-instance-${instanceId}"]`).click();

    await browser.waitUntil(
      async () => {
        const el = await browser.$('[data-testid="java-progress"]');
        return (await el.getText()).includes('Descargando Java');
      },
      { timeout: 60000, timeoutMsg: 'No apareció el indicador de descarga de Java' },
    );

    await browser.waitUntil(
      async () => {
        const text = await browser.$('[data-testid="java-progress"]').getText();
        return /[1-9][0-9]*\s*%/.test(text);
      },
      { timeout: 60000, timeoutMsg: 'No se observó progreso real >0% con Content-Length' },
    );

    await waitForText(`launch-message-${instanceId}`, 'Iniciado', 90000);

    await browser.waitUntil(() => fs.existsSync(pidFile), {
      timeout: 30000,
      timeoutMsg: 'El proceso Java fixture no escribió su PID',
    });
    const pid = Number(fs.readFileSync(pidFile, 'utf8').trim());
    assert.ok(Number.isInteger(pid) && pid > 1, `PID inválido: ${pid}`);
    assert.equal(processAlive(pid), true, 'El proceso Java debe estar vivo tras launchInstance');

    await browser.$(`[data-testid="kill-instance-${instanceId}"]`).click();
    await waitForText(`launch-message-${instanceId}`, 'detenida', 30000);
    await browser.waitUntil(() => !processAlive(pid), {
      timeout: 30000,
      timeoutMsg: `El PID ${pid} seguía vivo tras killInstance`,
    });
  });

  it('C — eliminar instancia borra config + carpeta y actualiza la lista', async () => {
    const dir = instanceDir(instanceId);
    assert.ok(fs.existsSync(dir), 'La carpeta debía existir antes de borrar');

    await browser.$(`[data-testid="delete-instance-${instanceId}"]`).click();
    await browser.$(`[data-testid="confirm-delete-${instanceId}"]`).click();

    await browser.waitUntil(async () => (await browser.$$(`[data-testid="instance-card-${instanceId}"]`)).length === 0, {
      timeout: 30000,
    });
    assert.equal(fs.existsSync(dir), false, 'La carpeta de la instancia debe desaparecer');
    assert.equal(fs.existsSync(path.join(dir, 'instance.json')), false, 'instance.json debe desaparecer');
    await waitForText('empty-instances', 'No hay instancias creadas', 30000);
  });
});
