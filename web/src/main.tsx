import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { ConfigurationProvider } from './ConfigurationProvider';
import { I18nProvider } from './i18n';
import './style.css';
import './desktop-analysis.css';
import './typography.css';
import './text-navigation.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <I18nProvider>
      <ConfigurationProvider>
        <App />
      </ConfigurationProvider>
    </I18nProvider>
  </React.StrictMode>,
);
