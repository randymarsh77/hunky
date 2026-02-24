import {useState, useCallback} from 'react';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header
      style={{
        padding: '4rem 0',
        textAlign: 'center',
        position: 'relative',
        overflow: 'hidden',
      }}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div style={{display: 'flex', gap: '1rem', justifyContent: 'center'}}>
          <Link
            className="button button--primary button--lg"
            to="/docs/intro">
            Get Started
          </Link>
          <Link
            className="button button--secondary button--lg"
            to="/coverage">
            Coverage Report
          </Link>
          <Link
            className="button button--secondary button--lg"
            to="/benchmarks">
            Benchmarks
          </Link>
        </div>
      </div>
    </header>
  );
}

const INSTALL_TYPES = [
  {id: 'binary', label: 'Prebuilt Binary'},
  {id: 'nix', label: 'Nix'},
  {id: 'source', label: 'From Source'},
];

const OS_OPTIONS = [
  {id: 'linux', label: 'Linux'},
  {id: 'macos', label: 'macOS'},
  {id: 'windows', label: 'Windows'},
];

const ARCH_OPTIONS = [
  {id: 'x86_64', label: 'x86_64'},
  {id: 'aarch64', label: 'aarch64'},
];

function getTarget(os, arch) {
  if (os === 'linux') return `${arch}-unknown-linux-gnu`;
  if (os === 'macos') return `${arch}-apple-darwin`;
  return `${arch}-pc-windows-msvc`;
}

function getBinaryCommands(os, arch) {
  const target = getTarget(os, arch);
  const base = 'https://github.com/randymarsh77/hunky/releases/latest/download';
  if (os === 'windows') {
    return [
      `Invoke-WebRequest -Uri ${base}/hunky-${target}.zip -OutFile hunky.zip`,
      'Expand-Archive hunky.zip -DestinationPath .',
    ];
  }
  return [
    `curl -sL ${base}/hunky-${target}.tar.gz | tar xz`,
    'sudo mv hunky /usr/local/bin/',
  ];
}

function getInstallCommands(type, os, arch) {
  if (type === 'binary') return getBinaryCommands(os, arch);
  if (type === 'nix') return ['nix run github:randymarsh77/hunky'];
  return ['cargo install --git https://github.com/randymarsh77/hunky'];
}

function CopyButton({text}) {
  const [copied, setCopied] = useState(false);
  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  }, [text]);
  return (
    <button
      className="install-copy-btn"
      onClick={handleCopy}
      title="Copy to clipboard"
      aria-label="Copy to clipboard">
      {copied ? '✓' : '⎘'}
    </button>
  );
}

function CommandLine({command}) {
  return (
    <div className="install-command-line">
      <code>{command}</code>
      <CopyButton text={command} />
    </div>
  );
}

function ButtonGroup({options, selected, onSelect}) {
  return (
    <div className="install-btn-group">
      {options.map((opt) => (
        <button
          key={opt.id}
          className={`button button--sm ${selected === opt.id ? 'button--primary' : 'button--outline button--secondary'}`}
          onClick={() => onSelect(opt.id)}>
          {opt.label}
        </button>
      ))}
    </div>
  );
}

function InstallSection() {
  const [installType, setInstallType] = useState('binary');
  const [os, setOs] = useState('linux');
  const [arch, setArch] = useState('x86_64');

  const commands = getInstallCommands(installType, os, arch);

  return (
    <section className="install-section">
      <div className="container">
        <Heading as="h2" style={{textAlign: 'center', marginBottom: '1.5rem'}}>
          Installation
        </Heading>
        <div className="install-controls">
          <ButtonGroup
            options={INSTALL_TYPES}
            selected={installType}
            onSelect={setInstallType}
          />
          {installType === 'binary' && (
            <div className="install-platform-controls">
              <ButtonGroup
                options={OS_OPTIONS}
                selected={os}
                onSelect={setOs}
              />
              <ButtonGroup
                options={ARCH_OPTIONS}
                selected={arch}
                onSelect={setArch}
              />
            </div>
          )}
        </div>
        <div className="install-commands">
          {commands.map((cmd) => (
            <CommandLine key={cmd} command={cmd} />
          ))}
        </div>
      </div>
    </section>
  );
}

export default function Home() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title="Home"
      description="Hunky — a TUI for observing git changes in real-time">
      <HomepageHeader />
      <InstallSection />
    </Layout>
  );
}
