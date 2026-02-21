#!/usr/bin/env node

/**
 * Process Criterion benchmark results into a JSON history file for the site.
 *
 * Usage:
 *   node process-benchmarks.mjs <criterion-dir> <output-dir> [site-url]
 *
 * - criterion-dir: path to downloaded Criterion output (e.g. benchmark-results)
 * - output-dir:    path where benchmark-data/history.json will be written
 * - site-url:      (optional) base URL of the deployed site to fetch existing history
 */

import fs from 'node:fs';
import path from 'node:path';

const [criterionDir, outputDir, siteUrl] = process.argv.slice(2);

if (!criterionDir || !outputDir) {
  console.error(
    'Usage: process-benchmarks.mjs <criterion-dir> <output-dir> [site-url]',
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------
// 1. Extract current benchmark data from Criterion estimates.json files
// ---------------------------------------------------------------------------
const benchmarks = {};

for (const name of fs.readdirSync(criterionDir)) {
  const estimatesPath = path.join(criterionDir, name, 'new', 'estimates.json');
  if (!fs.existsSync(estimatesPath)) continue;

  const data = JSON.parse(fs.readFileSync(estimatesPath, 'utf8'));
  benchmarks[name] = {
    mean: data.mean.point_estimate,
    median: data.median.point_estimate,
    std_dev: data.std_dev.point_estimate,
  };
}

const entry = {
  timestamp: new Date().toISOString(),
  commit: (process.env.GITHUB_SHA || 'local').substring(0, 7),
  benchmarks,
};

console.log(
  `Extracted ${Object.keys(benchmarks).length} benchmark(s): ${Object.keys(benchmarks).join(', ')}`,
);

// ---------------------------------------------------------------------------
// 2. Fetch existing history from the deployed site (best-effort)
// ---------------------------------------------------------------------------
let history = [];

if (siteUrl) {
  const historyUrl = `${siteUrl.replace(/\/$/, '')}/benchmark-data/history.json`;
  try {
    const resp = await fetch(historyUrl);
    if (resp.ok) {
      history = await resp.json();
      console.log(
        `Fetched ${history.length} existing history entries from ${historyUrl}`,
      );
    }
  } catch {
    console.log('No existing history found; starting fresh.');
  }
}

// ---------------------------------------------------------------------------
// 3. Append current entry and cap history length
// ---------------------------------------------------------------------------
const MAX_HISTORY = 50;
history.push(entry);
if (history.length > MAX_HISTORY) {
  history = history.slice(-MAX_HISTORY);
}

// ---------------------------------------------------------------------------
// 4. Write output
// ---------------------------------------------------------------------------
const outDir = path.join(outputDir, 'benchmark-data');
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(
  path.join(outDir, 'history.json'),
  JSON.stringify(history, null, 2),
);

console.log(`Wrote ${history.length} entries to ${path.join(outDir, 'history.json')}`);
