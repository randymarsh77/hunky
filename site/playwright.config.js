import {defineConfig} from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 60000,
  expect: {timeout: 15000},
  use: {baseURL: 'http://127.0.0.1:4173', headless: true},
  webServer: {
    command: 'npm run serve -- --host 127.0.0.1 --port 4173 --no-open',
    url: 'http://127.0.0.1:4173/hunky/',
    reuseExistingServer: !process.env.CI,
    timeout: 30000,
  },
});
