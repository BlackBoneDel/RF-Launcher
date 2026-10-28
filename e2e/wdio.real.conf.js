import { config as base } from './wdio.conf.js';

export const config = {
  ...base,
  specs: ['./specs/real-minecraft.e2e.js'],
  mochaOpts: { ui: 'bdd', timeout: 600000 },
  onPrepare: async () => {
    await base.onPrepare();
    delete process.env.RF_LAUNCHER_E2E_ADOPTIUM_BASE;
    delete process.env.RF_LAUNCHER_E2E_MINECRAFT_FIXTURE;
    delete process.env.RF_LAUNCHER_E2E_PID_FILE;
    process.env.RF_LAUNCHER_E2E_DISABLE_SYSTEM_JAVA = '1';
    console.log('E2E_REAL_MINECRAFT=1 E2E_ADOPTIUM=LIVE E2E_RESOLVER=LIVE');
  },
};
