// @ts-check

import {themes as prismThemes} from 'prism-react-renderer';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Hunky',
  tagline: 'Git changes, streamed in real-time 🔥',
  favicon: 'img/favicon.ico',

  future: {
    v4: true,
  },

  url: 'https://randymarsh77.github.io',
  baseUrl: '/hunky/',

  organizationName: 'randymarsh77',
  projectName: 'hunky',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          editUrl:
            'https://github.com/randymarsh77/hunky/tree/main/site/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'Hunky',
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'tutorialSidebar',
            position: 'left',
            label: 'Docs',
          },
          {
            to: '/coverage',
            label: 'Coverage',
            position: 'left',
          },
          {
            href: 'https://github.com/randymarsh77/hunky',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              {
                label: 'Getting Started',
                to: '/docs/intro',
              },
            ],
          },
          {
            title: 'More',
            items: [
              {
                label: 'Coverage Report',
                to: '/coverage',
              },
              {
                label: 'GitHub',
                href: 'https://github.com/randymarsh77/hunky',
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} Hunky. Built with Docusaurus.`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
        additionalLanguages: ['bash', 'rust', 'toml'],
      },
    }),
};

export default config;
