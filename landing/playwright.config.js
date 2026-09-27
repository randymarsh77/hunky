import {defineConfig} from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 60000,
  expect: {timeout: 15000},
  use: {baseURL: process.env.DEMO_URL ?? 'http://127.0.0.1:4174', headless: true},
  webServer: process.env.DEMO_URL ? undefined : {
    command: 'npm run serve',
    url: 'http://127.0.0.1:4174/',
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
  },
});
