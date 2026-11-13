import type { Locale } from './i18n';

const en = {
  title: 'Database backup',
  exportTitle: 'Export backup',
  exportHelp:
    'Download all match databases, notes, analyses, profiles and study progress as one file.',
  exporting: 'Preparing backup …',
  exported: 'Backup download started.',
  importTitle: 'Restore backup',
  importHelp:
    'Use a .bgtutor-backup file from a BG Tutor installation with the same database schema.',
  file: 'Backup file',
  chooseFile: 'Choose backup file',
  noFile: 'No file selected',
  review: 'Review database replacement',
  warning:
    'Restoring replaces all BG Tutor data in the destination installation. Export the current database first if you want to keep it. Queued or running work in the backup will be ended.',
  validation:
    'BG Tutor checks the backup file and its database schema after you confirm and before it replaces any data.',
  cancel: 'Cancel',
  replace: 'Replace database',
  restoring: 'Restoring backup …',
  done: 'Backup restored. Reload BG Tutor to open the restored database.',
  reload: 'Reload BG Tutor',
};
type Messages = Record<keyof typeof en, string>;

export const databaseBackupMessages = {
  en,
  de: {
    title: 'Datenbanksicherung',
    exportTitle: 'Sicherung exportieren',
    exportHelp:
      'Alle Partiedatenbanken, Notizen, Analysen, Profile und den Lernfortschritt als eine Datei herunterladen.',
    exporting: 'Sicherung wird erstellt …',
    exported: 'Der Download der Sicherung wurde gestartet.',
    importTitle: 'Sicherung wiederherstellen',
    importHelp:
      'Verwende eine .bgtutor-backup-Datei aus einer BG-Tutor-Installation mit demselben Datenbankschema.',
    file: 'Sicherungsdatei',
    chooseFile: 'Sicherungsdatei wählen',
    noFile: 'Keine Datei ausgewählt',
    review: 'Wiederherstellung ansehen',
    warning:
      'Die Wiederherstellung ersetzt den gesamten BG-Tutor-Datenbestand der Zielinstallation. Exportiere den aktuellen Bestand zuerst, wenn du ihn behalten möchtest. Wartende und laufende Aufträge aus der Sicherung werden beendet.',
    validation:
      'Nach deiner Bestätigung prüft BG Tutor die Sicherungsdatei und ihr Datenbankschema, bevor es Daten ersetzt.',
    cancel: 'Abbrechen',
    replace: 'Datenbank ersetzen',
    restoring: 'Sicherung wird eingespielt …',
    done: 'Die Sicherung wurde eingespielt. Lade BG Tutor neu, um den wiederhergestellten Bestand zu öffnen.',
    reload: 'BG Tutor neu laden',
  },
  es: {
    title: 'Copia de la base de datos',
    exportTitle: 'Exportar copia',
    exportHelp:
      'Descarga todas las bases de partidas, notas, análisis, perfiles y el progreso de estudio en un archivo.',
    exporting: 'Preparando la copia…',
    exported: 'Se ha iniciado la descarga de la copia.',
    importTitle: 'Restaurar copia',
    importHelp:
      'Usa un archivo .bgtutor-backup de una instalación de BG Tutor con el mismo esquema de base de datos.',
    file: 'Archivo de copia',
    chooseFile: 'Elegir archivo de copia',
    noFile: 'Ningún archivo seleccionado',
    review: 'Ver reemplazo de la base de datos',
    warning:
      'La restauración reemplaza todos los datos de BG Tutor en la instalación de destino. Exporta primero la base de datos actual si quieres conservarla. Las tareas pendientes o en curso de la copia se detendrán.',
    validation:
      'Tras tu confirmación, BG Tutor comprueba el archivo de copia y su esquema de base de datos antes de reemplazar los datos.',
    cancel: 'Cancelar',
    replace: 'Reemplazar base de datos',
    restoring: 'Restaurando la copia…',
    done: 'Copia restaurada. Recarga BG Tutor para abrir la base de datos restaurada.',
    reload: 'Recargar BG Tutor',
  },
  fr: {
    title: 'Sauvegarde de la base de données',
    exportTitle: 'Exporter une sauvegarde',
    exportHelp:
      'Téléchargez toutes les bases de parties, les notes, les analyses, les profils et la progression dans un seul fichier.',
    exporting: 'Préparation de la sauvegarde…',
    exported: 'Le téléchargement de la sauvegarde a démarré.',
    importTitle: 'Restaurer une sauvegarde',
    importHelp:
      'Utilisez un fichier .bgtutor-backup provenant d’une installation de BG Tutor ayant le même schéma de base de données.',
    file: 'Fichier de sauvegarde',
    chooseFile: 'Choisir le fichier de sauvegarde',
    noFile: 'Aucun fichier sélectionné',
    review: 'Voir le remplacement de la base',
    warning:
      'La restauration remplace toutes les données de BG Tutor dans l’installation de destination. Exportez d’abord la base actuelle si vous souhaitez la conserver. Les tâches en attente ou en cours dans la sauvegarde seront arrêtées.',
    validation:
      'Après votre confirmation, BG Tutor vérifie le fichier de sauvegarde et son schéma de base de données avant de remplacer les données.',
    cancel: 'Annuler',
    replace: 'Remplacer la base de données',
    restoring: 'Restauration de la sauvegarde…',
    done: 'Sauvegarde restaurée. Rechargez BG Tutor pour ouvrir la base restaurée.',
    reload: 'Recharger BG Tutor',
  },
  it: {
    title: 'Backup del database',
    exportTitle: 'Esporta backup',
    exportHelp:
      'Scarica in un unico file tutti i database delle partite, le note, le analisi, i profili e i progressi di studio.',
    exporting: 'Preparazione del backup…',
    exported: 'Il download del backup è iniziato.',
    importTitle: 'Ripristina backup',
    importHelp:
      'Usa un file .bgtutor-backup da un’installazione di BG Tutor con lo stesso schema del database.',
    file: 'File di backup',
    chooseFile: 'Scegli file di backup',
    noFile: 'Nessun file selezionato',
    review: 'Rivedi la sostituzione del database',
    warning:
      'Il ripristino sostituisce tutti i dati di BG Tutor nell’installazione di destinazione. Esporta prima il database attuale se vuoi conservarlo. Le attività in attesa o in corso nel backup saranno interrotte.',
    validation:
      'Dopo la conferma, BG Tutor verifica il file di backup e il suo schema prima di sostituire i dati.',
    cancel: 'Annulla',
    replace: 'Sostituisci database',
    restoring: 'Ripristino del backup…',
    done: 'Backup ripristinato. Ricarica BG Tutor per aprire il database ripristinato.',
    reload: 'Ricarica BG Tutor',
  },
  pl: {
    title: 'Kopia zapasowa bazy danych',
    exportTitle: 'Eksportuj kopię',
    exportHelp:
      'Pobierz wszystkie bazy meczów, notatki, analizy, profile i postępy w nauce w jednym pliku.',
    exporting: 'Przygotowywanie kopii…',
    exported: 'Rozpoczęto pobieranie kopii zapasowej.',
    importTitle: 'Przywróć kopię',
    importHelp:
      'Użyj pliku .bgtutor-backup z instalacji BG Tutor z tym samym schematem bazy danych.',
    file: 'Plik kopii zapasowej',
    chooseFile: 'Wybierz plik kopii zapasowej',
    noFile: 'Nie wybrano pliku',
    review: 'Zobacz szczegóły zastąpienia bazy',
    warning:
      'Przywrócenie zastąpi wszystkie dane BG Tutor w instalacji docelowej. Najpierw wyeksportuj obecną bazę, jeśli chcesz ją zachować. Oczekujące i trwające zadania z kopii zostaną przerwane.',
    validation:
      'Po potwierdzeniu BG Tutor sprawdzi plik kopii zapasowej i jego schemat bazy danych przed zastąpieniem danych.',
    cancel: 'Anuluj',
    replace: 'Zastąp bazę danych',
    restoring: 'Przywracanie kopii…',
    done: 'Kopia została przywrócona. Odśwież BG Tutor, aby otworzyć przywróconą bazę.',
    reload: 'Odśwież BG Tutor',
  },
} satisfies Record<Locale, Messages>;
