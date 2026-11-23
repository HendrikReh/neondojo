import { defineMessages } from '../../shared/i18n/messages';

const en = {
  title: 'Virtual profiles',
  kind: 'Virtual profile',
  create: 'New virtual profile …',
  createTitle: 'New virtual profile',
  name: 'Name',
  createAction: 'Create',
  nameTaken: 'A virtual profile with this name already exists.',
  empty:
    'A virtual profile groups real profiles, such as your accounts on several sites, without changing them.',
  me: 'My profile',
  members: 'Members',
  matchCount: '{count, plural, one {# match} other {# matches}}',
  memberCount: '{count, plural, one {# member} other {# members}}',
  noMembers:
    'No members yet. Add the real profiles that belong to this person.',
  addMember: 'Add member …',
  addMemberTitle: 'Add member',
  addMemberHint:
    'The real profile stays unchanged and keeps its own entry in the player list. Each profile belongs to at most one virtual profile.',
  memberSearch: 'Profile to add',
  add: 'Add',
  remove: 'Remove',
  removeMember: 'Remove {name} from {group}',
  otherGroup: 'Already in {name}; remove it there first.',
  alreadyMember: 'Already a member.',
  rename: 'Rename …',
  renameTitle: 'Rename virtual profile',
  save: 'Save',
  delete: 'Delete …',
  deleteTitle: 'Delete virtual profile',
  deleteHint:
    '“{name}” is deleted. Its member profiles, matches and analyses stay unchanged.',
  partOf: 'Part of',
  botMatches:
    'Your NeonDojo bot matches count once your NeonDojo player is a member. They count for the PR only when played in Competition mode without learning aids.',
  myProfile: 'My player profile',
  myProfileNone: 'None',
  myProfileHelp:
    'Study, My recurring mistakes, All analyzed decisions and the player database start with this virtual profile. Add your NeonDojo player as a member so your games against the bot count towards it.',
  myProfileUnavailable: 'Virtual profiles could not be loaded.',
  applyMyProfile: 'My player profile is saved with the player database.',
};
const messages = defineMessages('virtual-player', {
  en,
  de: {
    title: 'Virtuelle Profile',
    kind: 'Virtuelles Profil',
    create: 'Neues virtuelles Profil …',
    createTitle: 'Neues virtuelles Profil',
    name: 'Name',
    createAction: 'Anlegen',
    nameTaken: 'Ein virtuelles Profil mit diesem Namen gibt es bereits.',
    empty:
      'Ein virtuelles Profil fasst reale Profile zusammen, etwa deine Konten auf mehreren Plattformen, ohne sie zu verändern.',
    me: 'Mein Profil',
    members: 'Mitglieder',
    matchCount: '{count, plural, one {# Match} other {# Matches}}',
    memberCount: '{count, plural, one {# Mitglied} other {# Mitglieder}}',
    noMembers:
      'Noch keine Mitglieder. Füge die realen Profile hinzu, die zu dieser Person gehören.',
    addMember: 'Mitglied hinzufügen …',
    addMemberTitle: 'Mitglied hinzufügen',
    addMemberHint:
      'Das reale Profil bleibt unverändert und behält seinen eigenen Eintrag in der Spielerliste. Jedes Profil gehört zu höchstens einem virtuellen Profil.',
    memberSearch: 'Hinzuzufügendes Profil',
    add: 'Hinzufügen',
    remove: 'Entfernen',
    removeMember: '{name} aus {group} entfernen',
    otherGroup: 'Bereits in {name}; entferne es zuerst dort.',
    alreadyMember: 'Bereits Mitglied.',
    rename: 'Umbenennen …',
    renameTitle: 'Virtuelles Profil umbenennen',
    save: 'Speichern',
    delete: 'Löschen …',
    deleteTitle: 'Virtuelles Profil löschen',
    deleteHint:
      '„{name}“ wird gelöscht. Seine Mitgliedsprofile, Matches und Analysen bleiben unverändert.',
    partOf: 'Gehört zu',
    botMatches:
      'Deine NeonDojo-Partien gegen den Bot zählen, sobald dein NeonDojo-Spieler Mitglied ist. Für den PR zählen sie nur im Modus Competition ohne Lernhilfen.',
    myProfile: 'Mein Spielerprofil',
    myProfileNone: 'Keines',
    myProfileHelp:
      'Study, Meine wiederkehrenden Fehler, Alle analysierten Entscheidungen und die Spielerdatenbank beginnen mit diesem virtuellen Profil. Füge deinen NeonDojo-Spieler als Mitglied hinzu, damit deine Partien gegen den Bot dazuzählen.',
    myProfileUnavailable: 'Virtuelle Profile konnten nicht geladen werden.',
    applyMyProfile:
      'Mein Spielerprofil wird mit der Spielerdatenbank gespeichert.',
  },
  es: {
    title: 'Perfiles virtuales',
    kind: 'Perfil virtual',
    create: 'Nuevo perfil virtual …',
    createTitle: 'Nuevo perfil virtual',
    name: 'Nombre',
    createAction: 'Crear',
    nameTaken: 'Ya existe un perfil virtual con este nombre.',
    empty:
      'Un perfil virtual agrupa perfiles reales, como tus cuentas en varios sitios, sin modificarlos.',
    me: 'Mi perfil',
    members: 'Miembros',
    matchCount: '{count, plural, one {# encuentro} other {# encuentros}}',
    memberCount: '{count, plural, one {# miembro} other {# miembros}}',
    noMembers:
      'Todavía no hay miembros. Añade los perfiles reales que pertenecen a esta persona.',
    addMember: 'Añadir miembro …',
    addMemberTitle: 'Añadir miembro',
    addMemberHint:
      'El perfil real no cambia y conserva su propia entrada en la lista de jugadores. Cada perfil pertenece como máximo a un perfil virtual.',
    memberSearch: 'Perfil que añadir',
    add: 'Añadir',
    remove: 'Quitar',
    removeMember: 'Quitar {name} de {group}',
    otherGroup: 'Ya está en {name}; quítalo allí primero.',
    alreadyMember: 'Ya es miembro.',
    rename: 'Cambiar nombre …',
    renameTitle: 'Cambiar nombre del perfil virtual',
    save: 'Guardar',
    delete: 'Eliminar …',
    deleteTitle: 'Eliminar perfil virtual',
    deleteHint:
      'Se elimina «{name}». Sus perfiles miembros, encuentros y análisis no cambian.',
    partOf: 'Forma parte de',
    botMatches:
      'Tus encuentros de NeonDojo contra el bot cuentan cuando tu jugador de NeonDojo es miembro. Solo cuentan para el PR si se jugaron en modo Competición sin ayudas de aprendizaje.',
    myProfile: 'Mi perfil de jugador',
    myProfileNone: 'Ninguno',
    myProfileHelp:
      'Estudio, Mis errores recurrentes, Todas las decisiones analizadas y la base de datos de jugadores empiezan con este perfil virtual. Añade tu jugador de NeonDojo como miembro para que tus encuentros contra el bot cuenten para él.',
    myProfileUnavailable: 'No se pudieron cargar los perfiles virtuales.',
    applyMyProfile:
      'Mi perfil de jugador se guarda con la base de datos de jugadores.',
  },
  fr: {
    title: 'Profils virtuels',
    kind: 'Profil virtuel',
    create: 'Nouveau profil virtuel …',
    createTitle: 'Nouveau profil virtuel',
    name: 'Nom',
    createAction: 'Créer',
    nameTaken: 'Un profil virtuel porte déjà ce nom.',
    empty:
      'Un profil virtuel regroupe des profils réels, comme vos comptes sur plusieurs sites, sans les modifier.',
    me: 'Mon profil',
    members: 'Membres',
    matchCount: '{count, plural, one {# match} other {# matchs}}',
    memberCount: '{count, plural, one {# membre} other {# membres}}',
    noMembers:
      'Aucun membre pour l’instant. Ajoutez les profils réels qui appartiennent à cette personne.',
    addMember: 'Ajouter un membre …',
    addMemberTitle: 'Ajouter un membre',
    addMemberHint:
      'Le profil réel reste inchangé et garde sa propre entrée dans la liste des joueurs. Chaque profil appartient au plus à un profil virtuel.',
    memberSearch: 'Profil à ajouter',
    add: 'Ajouter',
    remove: 'Retirer',
    removeMember: 'Retirer {name} de {group}',
    otherGroup: 'Déjà dans {name} ; retirez-le d’abord de ce profil.',
    alreadyMember: 'Déjà membre.',
    rename: 'Renommer …',
    renameTitle: 'Renommer le profil virtuel',
    save: 'Enregistrer',
    delete: 'Supprimer …',
    deleteTitle: 'Supprimer le profil virtuel',
    deleteHint:
      '« {name} » est supprimé. Ses profils membres, matchs et analyses restent inchangés.',
    partOf: 'Fait partie de',
    botMatches:
      'Vos matchs NeonDojo contre le bot comptent dès que votre joueur NeonDojo est membre. Ils ne comptent pour le PR que s’ils ont été joués en mode Compétition sans aide à l’apprentissage.',
    myProfile: 'Mon profil de joueur',
    myProfileNone: 'Aucun',
    myProfileHelp:
      'Étude, Mes erreurs récurrentes, Toutes les décisions analysées et la base de joueurs commencent par ce profil virtuel. Ajoutez votre joueur NeonDojo comme membre pour que vos parties contre le bot y soient comptées.',
    myProfileUnavailable: 'Impossible de charger les profils virtuels.',
    applyMyProfile:
      'Mon profil de joueur est enregistré avec la base de joueurs.',
  },
  it: {
    title: 'Profili virtuali',
    kind: 'Profilo virtuale',
    create: 'Nuovo profilo virtuale …',
    createTitle: 'Nuovo profilo virtuale',
    name: 'Nome',
    createAction: 'Crea',
    nameTaken: 'Esiste già un profilo virtuale con questo nome.',
    empty:
      'Un profilo virtuale raggruppa profili reali, come i tuoi account su più siti, senza modificarli.',
    me: 'Il mio profilo',
    members: 'Membri',
    matchCount: '{count, plural, one {# incontro} other {# incontri}}',
    memberCount: '{count, plural, one {# membro} other {# membri}}',
    noMembers:
      'Ancora nessun membro. Aggiungi i profili reali che appartengono a questa persona.',
    addMember: 'Aggiungi membro …',
    addMemberTitle: 'Aggiungi membro',
    addMemberHint:
      'Il profilo reale resta invariato e mantiene la propria voce nell’elenco dei giocatori. Ogni profilo appartiene al massimo a un profilo virtuale.',
    memberSearch: 'Profilo da aggiungere',
    add: 'Aggiungi',
    remove: 'Rimuovi',
    removeMember: 'Rimuovi {name} da {group}',
    otherGroup: 'Già in {name}; rimuovilo prima da lì.',
    alreadyMember: 'Già membro.',
    rename: 'Rinomina …',
    renameTitle: 'Rinomina profilo virtuale',
    save: 'Salva',
    delete: 'Elimina …',
    deleteTitle: 'Elimina profilo virtuale',
    deleteHint:
      '«{name}» viene eliminato. I suoi profili membri, gli incontri e le analisi restano invariati.',
    partOf: 'Fa parte di',
    botMatches:
      'I tuoi incontri NeonDojo contro il bot contano quando il tuo giocatore NeonDojo è un membro. Contano per il PR solo se giocati in modalità Competizione senza aiuti didattici.',
    myProfile: 'Il mio profilo giocatore',
    myProfileNone: 'Nessuno',
    myProfileHelp:
      'Studio, I miei errori ricorrenti, Tutte le decisioni analizzate e il database giocatori partono da questo profilo virtuale. Aggiungi il tuo giocatore NeonDojo come membro perché le tue partite contro il bot vengano conteggiate qui.',
    myProfileUnavailable: 'Impossibile caricare i profili virtuali.',
    applyMyProfile:
      'Il mio profilo giocatore viene salvato con il database giocatori.',
  },
  pl: {
    title: 'Profile wirtualne',
    kind: 'Profil wirtualny',
    create: 'Nowy profil wirtualny …',
    createTitle: 'Nowy profil wirtualny',
    name: 'Nazwa',
    createAction: 'Utwórz',
    nameTaken: 'Profil wirtualny o tej nazwie już istnieje.',
    empty:
      'Profil wirtualny grupuje rzeczywiste profile, np. Twoje konta w kilku serwisach, nie zmieniając ich.',
    me: 'Mój profil',
    members: 'Członkowie',
    matchCount:
      '{count, plural, one {# mecz} few {# mecze} many {# meczów} other {# meczu}}',
    memberCount:
      '{count, plural, one {# członek} few {# członków} many {# członków} other {# członka}}',
    noMembers:
      'Brak członków. Dodaj rzeczywiste profile, które należą do tej osoby.',
    addMember: 'Dodaj członka …',
    addMemberTitle: 'Dodaj członka',
    addMemberHint:
      'Rzeczywisty profil pozostaje bez zmian i zachowuje własny wpis na liście graczy. Każdy profil należy do co najwyżej jednego profilu wirtualnego.',
    memberSearch: 'Profil do dodania',
    add: 'Dodaj',
    remove: 'Usuń',
    removeMember: 'Usuń {name} z {group}',
    otherGroup: 'Już w {name}; najpierw usuń go stamtąd.',
    alreadyMember: 'Już jest członkiem.',
    rename: 'Zmień nazwę …',
    renameTitle: 'Zmień nazwę profilu wirtualnego',
    save: 'Zapisz',
    delete: 'Usuń …',
    deleteTitle: 'Usuń profil wirtualny',
    deleteHint:
      '„{name}” zostanie usunięty. Jego profile członkowskie, mecze i analizy pozostają bez zmian.',
    partOf: 'Należy do',
    botMatches:
      'Twoje mecze NeonDojo z botem liczą się, gdy Twój gracz NeonDojo jest członkiem. Liczą się do PR tylko wtedy, gdy rozegrano je w trybie Rywalizacja bez pomocy w nauce.',
    myProfile: 'Mój profil gracza',
    myProfileNone: 'Brak',
    myProfileHelp:
      'Nauka, Moje powtarzające się błędy, Wszystkie przeanalizowane decyzje i baza graczy zaczynają od tego profilu wirtualnego. Dodaj swojego gracza NeonDojo jako członka, aby Twoje partie z botem były do niego wliczane.',
    myProfileUnavailable: 'Nie udało się wczytać profili wirtualnych.',
    applyMyProfile: 'Mój profil gracza jest zapisywany w bazie graczy.',
  },
});
export const useVirtualPlayerText = messages.useText;
