import {
  defineMessages,
  fromRows,
  type MessageRow,
} from '../../shared/i18n/messages';

const entries = {
  tab: ['Versions', 'Versionen', 'Versiones', 'Versions', 'Wersje', 'Versioni'],
  available: [
    'Available',
    'Verfügbar',
    'Disponible',
    'Disponible',
    'Dostępny',
    'Disponibile',
  ],
  details: [
    'Technical details',
    'Technische Details',
    'Detalles técnicos',
    'Détails techniques',
    'Szczegóły techniczne',
    'Dettagli tecnici',
  ],
  appVersion: [
    'NeonDojo version',
    'NeonDojo-Version',
    'Versión de NeonDojo',
    'Version de NeonDojo',
    'Wersja NeonDojo',
    'Versione di NeonDojo',
  ],
  appCommit: [
    'NeonDojo commit',
    'NeonDojo-Commit',
    'Commit de NeonDojo',
    'Commit de NeonDojo',
    'Commit NeonDojo',
    'Commit di NeonDojo',
  ],
  copy: [
    'Copy version information',
    'Versionsinformationen kopieren',
    'Copiar información de versiones',
    'Copier les informations sur les versions',
    'Kopiuj informacje o wersjach',
    'Copia le informazioni sulle versioni',
  ],
  copied: [
    'Version information copied.',
    'Versionsinformationen kopiert.',
    'Se ha copiado la información de versiones.',
    'Informations sur les versions copiées.',
    'Skopiowano informacje o wersjach.',
    'Informazioni sulle versioni copiate.',
  ],
  copyError: [
    'Version information could not be copied.',
    'Versionsinformationen konnten nicht kopiert werden.',
    'No se pudo copiar la información de versiones.',
    'Impossible de copier les informations sur les versions.',
    'Nie udało się skopiować informacji o wersjach.',
    'Impossibile copiare le informazioni sulle versioni.',
  ],
} satisfies Record<string, MessageRow>;

const messages = defineMessages('version-info', fromRows(entries));
export const versionInfoMessages = messages.catalogs;
export const versionInfoText = messages.text;
export const useVersionInfoText = messages.useText;
