// First: the stylesheets, in cascade order (see stylesheets.ts).
import './styles/stylesheets';
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './app/App';
import { ConfigurationProvider } from './settings/configuration/ConfigurationProvider';
import { I18nProvider } from './shared/i18n/i18n';
import { readLocale } from './shared/i18n/i18n-core';
import { preloadStartupAssets } from './startup-preload';

const initialLocale = readLocale();
document.documentElement.lang = initialLocale;
const root = document.getElementById('root')!;

async function start() {
  await preloadStartupAssets(initialLocale);
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
