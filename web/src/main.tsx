// First: the stylesheets, in cascade order (see stylesheets.ts).
import './styles/stylesheets';
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './app/App';
import { ConfigurationProvider } from './settings/configuration/ConfigurationProvider';
import { I18nProvider } from './shared/i18n/i18n';
import { loadLocale } from './shared/i18n/i18n-catalogs';
import { readLocale } from './shared/i18n/i18n-core';
import { loadArchiveThemes } from './shared/board/themes';
import { readTheme } from './settings/ui-preferences';

const initialLocale = readLocale();
document.documentElement.lang = initialLocale;
const root = document.getElementById('root')!;

async function start() {
  await Promise.all([
    loadLocale(initialLocale),
    readTheme().startsWith('bgblitz:') && loadArchiveThemes(),
  ]);
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <I18nProvider initialLocale={initialLocale}>
        <ConfigurationProvider>
          <App />
        </ConfigurationProvider>
      </I18nProvider>
    </React.StrictMode>,
  );
}

void start().catch(() => {
  // Even if the first language chunk is unavailable, allow a fresh deployment to load.
  const reload = document.createElement('button');
  reload.textContent = '↻ NeonDojo';
  reload.onclick = () => window.location.reload();
  root.replaceChildren(reload);
});
