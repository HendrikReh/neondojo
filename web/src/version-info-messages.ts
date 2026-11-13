import type { Locale } from './i18n';

const en = {
  available: 'Available',
  details: 'Technical details',
  appCommit: 'BG Tutor commit',
  copy: 'Copy version information',
  copied: 'Version information copied.',
  copyError: 'Version information could not be copied.',
};

type Messages = Record<keyof typeof en, string>;

export const versionInfoMessages = {
  en,
  de: {
    available: 'Verfügbar',
    details: 'Technische Details',
    appCommit: 'BG-Tutor-Commit',
    copy: 'Versionsinformationen kopieren',
    copied: 'Versionsinformationen kopiert.',
    copyError: 'Versionsinformationen konnten nicht kopiert werden.',
  },
  es: {
    available: 'Disponible',
    details: 'Detalles técnicos',
    appCommit: 'Commit de BG Tutor',
    copy: 'Copiar información de versiones',
    copied: 'Se ha copiado la información de versiones.',
    copyError: 'No se pudo copiar la información de versiones.',
  },
  fr: {
    available: 'Disponible',
    details: 'Détails techniques',
    appCommit: 'Commit de BG Tutor',
    copy: 'Copier les informations sur les versions',
    copied: 'Informations sur les versions copiées.',
    copyError: 'Impossible de copier les informations sur les versions.',
  },
  it: {
    available: 'Disponibile',
    details: 'Dettagli tecnici',
    appCommit: 'Commit di BG Tutor',
    copy: 'Copia le informazioni sulle versioni',
    copied: 'Informazioni sulle versioni copiate.',
    copyError: 'Impossibile copiare le informazioni sulle versioni.',
  },
  pl: {
    available: 'Dostępny',
    details: 'Szczegóły techniczne',
    appCommit: 'Commit BG Tutor',
    copy: 'Kopiuj informacje o wersjach',
    copied: 'Skopiowano informacje o wersjach.',
    copyError: 'Nie udało się skopiować informacji o wersjach.',
  },
} satisfies Record<Locale, Messages>;
