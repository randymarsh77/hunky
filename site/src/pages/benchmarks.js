import Layout from '@theme/Layout';
import useBaseUrl from '@docusaurus/useBaseUrl';
import {useState, useEffect} from 'react';

/** Format nanoseconds into a human-friendly string. */
function formatNs(ns) {
  if (ns >= 1_000_000_000) return `${(ns / 1_000_000_000).toFixed(2)} s`;
  if (ns >= 1_000_000) return `${(ns / 1_000_000).toFixed(2)} ms`;
  if (ns >= 1_000) return `${(ns / 1_000).toFixed(2)} µs`;
  return `${ns.toFixed(0)} ns`;
}

/** Tiny sparkline SVG rendered from an array of trend entries. */
function Sparkline({entries, width = 220, height = 40}) {
  if (!entries || entries.length < 2) return null;
  const values = entries.map((e) => e.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;
  const points = values
    .map((v, i) => {
      const x = (i / (values.length - 1)) * width;
      const y = height - ((v - min) / range) * (height - 4) - 2;
      return `${x},${y}`;
    })
    .join(' ');

  return (
    <svg
      width={width}
      height={height}
      viewBox={`0 0 ${width} ${height}`}
      style={{display: 'block'}}>
      <polyline
        fill="none"
        stroke="var(--ifm-color-primary)"
        strokeWidth="2"
        points={points}
      />
      {/* dot on latest value */}
      {(() => {
        const last = values.length - 1;
        const cx = width;
        const cy =
          height - ((values[last] - min) / range) * (height - 4) - 2;
        return <circle cx={cx} cy={cy} r="3" fill="var(--ifm-color-primary)" />;
      })()}
    </svg>
  );
}

/** Change badge showing the delta between the two most recent entries. */
function ChangeBadge({entries}) {
  if (!entries || entries.length < 2) return null;
  const prev = entries[entries.length - 2].value;
  const curr = entries[entries.length - 1].value;
  const pct = ((curr - prev) / prev) * 100;
  const improved = pct < 0;
  const color = improved ? '#2e8555' : pct === 0 ? '#888' : '#d9534f';
  const arrow = improved ? '▼' : pct === 0 ? '–' : '▲';
  return (
    <span
      style={{
        color,
        fontWeight: 600,
        fontSize: '0.85rem',
        whiteSpace: 'nowrap',
      }}>
      {arrow} {Math.abs(pct).toFixed(1)}%
    </span>
  );
}

export default function Benchmarks() {
  const historyUrl = useBaseUrl('/benchmark-data/history.json');
  const [history, setHistory] = useState(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    fetch(historyUrl)
      .then((r) => {
        if (!r.ok) throw new Error(r.statusText);
        return r.json();
      })
      .then(setHistory)
      .catch(() => setError(true));
  }, [historyUrl]);

  const latest = history?.[history.length - 1];
  const benchmarkNames = latest ? Object.keys(latest.benchmarks).sort() : [];

  // Build per-benchmark trend arrays with their corresponding history indices
  const trends = {};
  if (history) {
    for (const name of benchmarkNames) {
      const entries = [];
      for (let i = 0; i < history.length; i++) {
        const v = history[i].benchmarks[name]?.mean;
        if (v != null) entries.push({value: v, index: i});
      }
      trends[name] = entries;
    }
  }

  return (
    <Layout title="Benchmarks" description="Benchmark results for Hunky">
      <div style={{padding: '2rem', maxWidth: 1100, margin: '0 auto'}}>
        <h1>Benchmarks</h1>
        <p>
          Performance benchmarks powered by{' '}
          <a
            href="https://bheisler.github.io/criterion.rs/book/"
            target="_blank"
            rel="noopener noreferrer">
            Criterion.rs
          </a>
          . Results are collected automatically on every CI run.
        </p>

        {error && (
          <div
            className="alert alert--warning"
            role="alert"
            style={{marginBottom: '1.5rem'}}>
            Benchmark data is not yet available. It will appear after the first
            CI run that includes benchmarks.
          </div>
        )}

        {history && latest && (
          <>
            {/* ---- Current numbers table ---- */}
            <h2>Latest Results</h2>
            <p style={{fontSize: '0.9rem', color: 'var(--ifm-color-emphasis-600)'}}>
              Commit{' '}
              <code>{latest.commit}</code> —{' '}
              {new Date(latest.timestamp).toLocaleString()}
            </p>
            <div style={{overflowX: 'auto'}}>
              <table>
                <thead>
                  <tr>
                    <th>Benchmark</th>
                    <th style={{textAlign: 'right'}}>Mean</th>
                    <th style={{textAlign: 'right'}}>Median</th>
                    <th style={{textAlign: 'right'}}>Std Dev</th>
                    <th style={{textAlign: 'center'}}>Change</th>
                    <th>Trend</th>
                  </tr>
                </thead>
                <tbody>
                  {benchmarkNames.map((name) => {
                    const b = latest.benchmarks[name];
                    return (
                      <tr key={name}>
                        <td>
                          <code>{name}</code>
                        </td>
                        <td style={{textAlign: 'right', fontVariantNumeric: 'tabular-nums'}}>
                          {formatNs(b.mean)}
                        </td>
                        <td style={{textAlign: 'right', fontVariantNumeric: 'tabular-nums'}}>
                          {formatNs(b.median)}
                        </td>
                        <td style={{textAlign: 'right', fontVariantNumeric: 'tabular-nums'}}>
                          {formatNs(b.std_dev)}
                        </td>
                        <td style={{textAlign: 'center'}}>
                          <ChangeBadge entries={trends[name]} />
                        </td>
                        <td>
                          <Sparkline entries={trends[name]} />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>

            {/* ---- Trend detail per benchmark ---- */}
            {history.length > 1 && (
              <>
                <h2 style={{marginTop: '2.5rem'}}>Trends</h2>
                <p style={{fontSize: '0.9rem', color: 'var(--ifm-color-emphasis-600)'}}>
                  Mean execution time over the last {history.length} CI runs.
                </p>
                <div
                  style={{
                    display: 'grid',
                    gridTemplateColumns: 'repeat(auto-fill, minmax(340px, 1fr))',
                    gap: '1.5rem',
                  }}>
                  {benchmarkNames.map((name) => (
                    <TrendCard
                      key={name}
                      name={name}
                      history={history}
                      entries={trends[name]}
                    />
                  ))}
                </div>
              </>
            )}
          </>
        )}
      </div>
    </Layout>
  );
}

/** Card showing a larger trend chart for a single benchmark. */
function TrendCard({name, history, entries}) {
  if (!entries || entries.length < 2) return null;

  const values = entries.map((e) => e.value);
  const firstCommit = history[entries[0].index]?.commit;
  const lastCommit = history[entries[entries.length - 1].index]?.commit;

  const width = 320;
  const height = 120;
  const padX = 40;
  const padY = 20;
  const innerW = width - padX;
  const innerH = height - padY * 2;

  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;

  const points = values
    .map((v, i) => {
      const x = padX + (i / (values.length - 1)) * innerW;
      const y = padY + innerH - ((v - min) / range) * innerH;
      return `${x},${y}`;
    })
    .join(' ');

  // Y-axis labels
  const yLabels = [min, (min + max) / 2, max];

  return (
    <div
      style={{
        border: '1px solid var(--ifm-toc-border-color)',
        borderRadius: 8,
        padding: '1rem',
      }}>
      <h4 style={{marginBottom: '0.5rem'}}>
        <code>{name}</code>
      </h4>
      <svg
        width="100%"
        viewBox={`0 0 ${width} ${height}`}
        style={{display: 'block'}}>
        {/* grid lines */}
        {yLabels.map((v, i) => {
          const y = padY + innerH - ((v - min) / range) * innerH;
          return (
            <g key={i}>
              <line
                x1={padX}
                y1={y}
                x2={width}
                y2={y}
                stroke="var(--ifm-toc-border-color)"
                strokeDasharray="3,3"
              />
              <text
                x={padX - 4}
                y={y + 4}
                textAnchor="end"
                fontSize="9"
                fill="var(--ifm-color-emphasis-600)">
                {formatNs(v)}
              </text>
            </g>
          );
        })}
        <polyline
          fill="none"
          stroke="var(--ifm-color-primary)"
          strokeWidth="2"
          points={points}
        />
        {/* dots */}
        {values.map((v, i) => {
          const cx = padX + (i / (values.length - 1)) * innerW;
          const cy = padY + innerH - ((v - min) / range) * innerH;
          return (
            <circle
              key={i}
              cx={cx}
              cy={cy}
              r="3"
              fill="var(--ifm-color-primary)"
            />
          );
        })}
      </svg>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          fontSize: '0.75rem',
          color: 'var(--ifm-color-emphasis-600)',
          marginTop: 4,
        }}>
        <span>{firstCommit}</span>
        <span>{lastCommit}</span>
      </div>
    </div>
  );
}
