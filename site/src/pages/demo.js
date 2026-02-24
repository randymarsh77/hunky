import Layout from '@theme/Layout';
import useBaseUrl from '@docusaurus/useBaseUrl';

export default function Demo() {
  const demoUrl = useBaseUrl('/demo-app/index.html');
  return (
    <Layout title="Demo" description="Interactive web demo of hunky">
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
          <h1 style={{marginBottom: '0.25rem'}}>Interactive Demo</h1>
          <p style={{marginBottom: 0}}>
            A live web demo of hunky running entirely in your browser via
            WebAssembly.  Click the terminal below and use <code>j</code>/<code>k</code> to
            navigate hunks, <code>J</code>/<code>K</code> for files, and <code>H</code> for
            help.
          </p>
        </div>
        <iframe
          src={demoUrl}
          title="Hunky Web Demo"
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
