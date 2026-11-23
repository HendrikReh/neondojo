// First: the stylesheets, in cascade order (see stylesheets.ts).
import './styles/stylesheets';
import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './app/App';
import { ConfigurationProvider } from './settings/configuration/ConfigurationProvider';
import { I18nProvider } from './shared/i18n/i18n';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <I18nProvider>
      <ConfigurationProvider>
        <App />
      </ConfigurationProvider>
    </I18nProvider>
  </React.StrictMode>,
);
