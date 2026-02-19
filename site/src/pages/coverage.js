import Layout from '@theme/Layout';
import useBaseUrl from '@docusaurus/useBaseUrl';

export default function Coverage() {
  const coverageUrl = useBaseUrl('/coverage-report/index.html');
  return (
    <Layout title="Coverage" description="Code coverage report for Hunky">
      <div
        style={{
          display: 'flex',
          flexDirection: 'column',
          height: 'calc(100vh - 60px)',
        }}>
        <div
          style={{
            padding: '1rem 2rem',
            borderBottom: '1px solid var(--ifm-toc-border-color)',
          }}>
          <h1 style={{marginBottom: '0.25rem'}}>Code Coverage</h1>
          <p style={{marginBottom: 0}}>
            Interactive coverage report generated from the latest CI run.
            Drill into files and lines to see what is covered.
          </p>
        </div>
        <iframe
          src={coverageUrl}
          title="Coverage Report"
          style={{
            flex: 1,
            width: '100%',
            border: 'none',
          }}
        />
      </div>
    </Layout>
  );
}
