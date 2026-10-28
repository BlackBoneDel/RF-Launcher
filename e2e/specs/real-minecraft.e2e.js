import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

const root = path.join(process.env.XDG_DATA_HOME, 'io.github.blackbonedel.rflauncher');
const instances = path.join(root, 'instances');
const selector = (name) => `[data-testid="${name}"]`;

describe('Minecraft Fabric real — UI, JVM y PID', () => {
  it('intenta lanzar Fabric con JVM gestionada y detener el PID real', async () => {
    let id;
    let pid;
    let observed;
    let observer;
    try {
      await browser.$(selector('new-instance')).waitForDisplayed({ timeout: 60000 });
      await browser.$(selector('new-instance')).click();
      await browser.$(selector('instance-name')).setValue('Real Fabric E2E');
      await browser.$(selector('minecraft-version')).setValue('1.21.1');
      const fabric = await browser.$(selector('use-fabric'));
      if (!await fabric.isSelected()) await fabric.click();
      await browser.$(selector('fabric-version')).setValue('0.16.14');
      await browser.$(selector('create-instance-submit')).click();
      await browser.waitUntil(() => fs.existsSync(instances) && fs.readdirSync(instances).some(n => /^[0-9a-f-]{36}$/.test(n)), { timeout: 30000 });
      id = fs.readdirSync(instances).find(n => /^[0-9a-f-]{36}$/.test(n));
      const config = JSON.parse(fs.readFileSync(path.join(instances, id, 'instance.json')));
      console.log(`E2E_INSTANCE=${JSON.stringify(config)}`);
      assert.equal(config.minecraftVersion, '1.21.1');
      assert.equal(config.fabricLoader, '0.16.14');
      observer = setInterval(() => {
        for (const entry of fs.readdirSync('/proc')) {
          if (!/^\d+$/.test(entry) || observed) continue;
          try {
            const exe = fs.readlinkSync(`/proc/${entry}/exe`);
            if (!exe.includes('/java/jdk-21/bin/java')) continue;
            const args = fs.readFileSync(`/proc/${entry}/cmdline`).toString().split('\0').filter(Boolean);
            if (args.includes('net.fabricmc.loader.impl.launch.knot.KnotClient')) {
              observed = { pid: Number(entry), exe, args };
              console.log(`E2E_OBSERVED_JVM=${JSON.stringify(observed)}`);
            }
          } catch { /* Process may exit during inspection. */ }
        }
      }, 25);
      await browser.$(selector(`launch-instance-${id}`)).click();
      let text = '';
      await browser.waitUntil(async () => {
        const msg = await browser.$(selector(`launch-message-${id}`));
        if (!await msg.isExisting()) return false;
        text = await msg.getText();
        return text.length > 0;
      }, { timeout: 480000, timeoutMsg: 'No hubo resultado de lanzamiento real en 8 minutos' });
      console.log(`E2E_LAUNCH_MESSAGE=${text}`);
      const match = text.match(/PID (\d+)/);
      assert.ok(match, `No se lanzó JVM: ${text}`);
      pid = Number(match[1]);
      const proc = `/proc/${pid}`;
      assert.ok(observed && observed.pid === pid, 'No se observó el proceso JVM con el PID reportado');
      const { exe, args } = observed;
      console.log(`E2E_JVM_PID=${pid} EXE=${exe}`);
      console.log(`E2E_JVM_ARGS=${JSON.stringify(args)}`);
      assert.equal(path.basename(exe), 'java');
      assert.ok(exe.includes('/java/jdk-21/'), 'No es el JDK gestionado por JavaManager');
      const cpIndex = args.indexOf('-cp');
      assert.ok(cpIndex >= 0);
      assert.ok(args[cpIndex + 1].includes('fabric-loader-0.16.14.jar'));
      assert.ok(args[cpIndex + 1].includes('asm-9.8.jar'));
      assert.ok(args.includes('net.fabricmc.loader.impl.launch.knot.KnotClient'));
      const logs = path.join(instances, id, 'logs');
      await browser.waitUntil(() => fs.readdirSync(logs).some(n => fs.statSync(path.join(logs, n)).size > 0), { timeout: 30000, timeoutMsg: 'JVM sin log real' });
      for (const file of fs.readdirSync(logs)) {
        const content = fs.readFileSync(path.join(logs, file), 'utf8');
        if (content) {
          console.log(`E2E_JVM_LOG=${content.slice(0, 3000)}`);
          assert.ok(/Loading Minecraft/i.test(content), 'No hay log de arranque de Fabric; un error JVM no demuestra arranque');
        }
      }
      const state = fs.readFileSync(`${proc}/stat`, 'utf8').match(/^\d+ \(.*\) (\S)/)?.[1];
      assert.ok(state && state !== 'Z', `JVM no está viva antes de kill: state=${state}`);
      console.log(`E2E_JVM_STATE_BEFORE_KILL=${state}`);
      await browser.$(selector(`kill-instance-${id}`)).click();
      await browser.waitUntil(async () => (await browser.$(selector(`launch-message-${id}`)).getText()).includes('detenida'), { timeout: 30000 });
      await browser.waitUntil(() => !fs.existsSync(proc), { timeout: 30000 });
      console.log(`E2E_JVM_KILL_PID_GONE=${pid}`);
      pid = undefined;
    } finally {
      clearInterval(observer);
      if (id) {
        const logs = path.join(instances, id, 'logs');
        if (fs.existsSync(logs)) for (const file of fs.readdirSync(logs)) {
          const content = fs.readFileSync(path.join(logs, file), 'utf8');
          if (content) console.log(`E2E_DIAGNOSTIC_LOG=${content.slice(0, 6000)}`);
        }
      }
      const cleanupPid = pid || observed?.pid;
      if (cleanupPid && fs.existsSync(`/proc/${cleanupPid}`)) {
        try {
          const exe = fs.readlinkSync(`/proc/${cleanupPid}/exe`);
          if (exe.includes('/java/jdk-21/bin/java')) {
            process.kill(cleanupPid, 'SIGKILL');
            console.log(`E2E_CLEANUP_SIGKILL=${cleanupPid}`);
          }
        } catch (error) { if (error.code !== 'ESRCH' && error.code !== 'ENOENT') throw error; }
      }
    }
  });
});
