/**
 * The message catalogue.
 *
 * The real catalogue is ICU-based and shared with the Rust backend; it ships for 14
 * locales from M1. This module is the client half of that pipeline and it already encodes
 * the two rules that are expensive to retrofit later:
 *
 * 1. a missing string falls back to English, never to the key and never to a blank;
 * 2. direction is derived from the locale, not from the text.
 *
 * Every user-visible string of the web client goes through `t()`. A hardcoded label is a
 * bug waiting for the locale switch.
 */

export type Locale = "en" | "fr" | "he";

export type Direction = "ltr" | "rtl";

/** Locales the M0 shell can render. `he` is here as the RTL proof, not as a shipped translation. */
export const SUPPORTED_LOCALES: readonly Locale[] = ["en", "fr", "he"] as const;

const CATALOGUES: Record<Locale, Readonly<Record<string, string>>> = {
  en: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "Threads",
    "shell.nav.activity": "Activity",
    "shell.nav.settings": "Settings",
    "shell.nav.primary": "Primary",
    "shell.placeholder": "Pick a bot in the rail to open its conversation.",
    "shell.rail.bots": "Bots",
    "shell.rail.activity": "Activity",
    "shell.rail.primary": "Workspace",
    "shell.rail.bots.empty": "No bot yet.",
    "shell.rail.snoozed": "Snoozed",
    "shell.rail.unsnooze": "Unsnooze",
    "thread.snooze": "Snooze this conversation",
    "thread.unsnooze": "End the snooze",
    "landing.title": "Persistent AI teammates, on hardware you own.",
    "landing.body":
      "This build carries the shell only. The thread, the run timeline and the computer panel arrive milestone by milestone; the API they talk to is already specified.",
    "landing.cta": "Open the app shell",
    "notfound.title": "Page not found",
    "notfound.body":
      "This address does not match a screen. It may have moved, or it may never have existed.",
    "notfound.back": "Back to the start",
    "common.retry": "Try again",
    "thread.back": "Back to the threads",
    "thread.transcript": "Conversation",
    "thread.reply.banner": "Replying to",
    "thread.reply.clear": "Cancel the reply",
    "thread.reply.button": "Reply",
    "thread.reactions.add": "Add a reaction",
    "thread.reactions.toggle": "React with",
    "thread.reactions.picker": "Choose an emoji",
    "thread.card.approve": "Approve",
    "thread.card.answer": "Answer",
    "thread.bot.active": "Active",
    "thread.bot.archived": "Archived",
    "thread.role.user": "You",
    "thread.role.bot": "Bot",
    "thread.role.system": "System",
    "thread.role.tool": "Tool",
    "thread.unread.label": "unread messages",
    "thread.block.image": "Image",
    "thread.block.file": "File",
    "thread.block.tool": "Tool call",
    "thread.block.card": "Card",
    "thread.tool.running": "Running",
    "thread.tool.finished": "Finished",
    "thread.loading": "Loading the conversation",
    "thread.error.load": "The conversation could not be loaded.",
    "thread.error.send": "Your message was not sent.",
    "thread.empty.intro": "I am ready. Ask me anything and I will get to work.",
    "thread.empty.suggestions": "Suggested prompts",
    "thread.empty.suggestion.1": "Plan my week",
    "thread.empty.suggestion.2": "Summarise my notes",
    "thread.composer.label": "Message",
    "thread.composer.placeholder": "Write a message",
    "thread.composer.send": "Send",
    "thread.composer.sending": "Sending",
    "thread.offline": "Not live: messages still send.",
    "thread.live": "Live",
    "thread.run.label": "Run",
    "activity.empty": "No run yet.",
    "activity.loading": "Loading the activity",
    "activity.error": "The activity could not be loaded.",
    "run.status.queued": "Waiting to start",
    "run.status.leased": "Claimed by a worker",
    "run.status.running": "Working",
    "run.status.waiting_input": "Waiting for your answer",
    "run.status.waiting_takeover": "Waiting for you to take over",
    "run.status.completed": "Done",
    "run.status.failed": "Failed",
    "run.status.cancelled": "Stopped",
    "run.trigger.user": "From a message",
    "run.trigger.routine": "From a routine",
    "run.trigger.webhook": "From a webhook",
    "run.trigger.bot_message": "From another bot",
    "run.trigger.tickets": "Ticket work",
    "run.trigger.created": "At creation",
    "run.trigger.helper_result": "Helper result",
    "run.trigger.retry": "Retry",
  },
  fr: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "Fils",
    "shell.nav.activity": "Activité",
    "shell.nav.primary": "Navigation principale",
    "shell.placeholder": "Choisis un bot dans la colonne de gauche pour ouvrir sa conversation.",
    "shell.rail.bots": "Bots",
    "shell.rail.activity": "Activité",
    "shell.rail.primary": "Espace de travail",
    "shell.rail.bots.empty": "Aucun bot pour l'instant.",
    "shell.rail.snoozed": "En veille",
    "shell.rail.unsnooze": "Réactiver",
    "thread.snooze": "Mettre cette conversation en veille",
    "thread.unsnooze": "Fin de la mise en veille",
    "landing.title": "Des colocataires IA persistants, sur ta propre machine.",
    "landing.body":
      "Cette version ne contient que la coque. Le fil, la chronologie des exécutions et le panneau computer arrivent jalon par jalon ; l'API qu'ils interrogent est déjà spécifiée.",
    "landing.cta": "Ouvrir la coque de l'application",
    "notfound.title": "Page introuvable",
    "notfound.body":
      "Cette adresse ne correspond à aucun écran. Elle a peut-être été déplacée, ou n'a jamais existé.",
    "notfound.back": "Revenir au début",
    "common.retry": "Réessayer",
    "thread.back": "Retour aux fils",
    "thread.reply.banner": "Réponse à",
    "thread.reply.clear": "Annuler la réponse",
    "thread.reply.button": "Répondre",
    "thread.reactions.add": "Ajouter une réaction",
    "thread.reactions.toggle": "Réagir avec",
    "thread.reactions.picker": "Choisir un emoji",
    "thread.card.approve": "Approuver",
    "thread.card.answer": "Répondre",
    "thread.transcript": "Conversation",
    "thread.bot.active": "Actif",
    "thread.bot.archived": "Archivé",
    "thread.role.user": "Vous",
    "thread.role.bot": "Bot",
    "thread.role.system": "Système",
    "thread.role.tool": "Outil",
    "thread.unread.label": "messages non lus",
    "thread.block.image": "Image",
    "thread.block.file": "Fichier",
    "thread.block.tool": "Appel d'outil",
    "thread.block.card": "Carte",
    "thread.tool.running": "En cours",
    "thread.tool.finished": "Terminé",
    "thread.loading": "Chargement de la conversation",
    "thread.error.load": "La conversation n'a pas pu être chargée.",
    "thread.error.send": "Ton message n'a pas été envoyé.",
    "thread.empty.intro": "Je suis prêt. Pose-moi une question et je me mets au travail.",
    "thread.empty.suggestions": "Questions suggérées",
    "thread.empty.suggestion.1": "Planifie ma semaine",
    "thread.empty.suggestion.2": "Résume mes notes",
    "thread.composer.label": "Message",
    "thread.composer.placeholder": "Écris un message",
    "thread.composer.send": "Envoyer",
    "thread.composer.sending": "Envoi",
    "thread.offline": "Pas en direct : les messages partent quand même.",
    "thread.live": "En direct",
    "thread.run.label": "Exécution",
    "activity.empty": "Aucune exécution pour l'instant.",
    "activity.loading": "Chargement de l'activité",
    "activity.error": "L'activité n'a pas pu être chargée.",
    "run.status.queued": "En attente de démarrage",
    "run.status.leased": "Prise en charge par un agent",
    "run.status.running": "En cours",
    "run.status.waiting_input": "En attente de ta réponse",
    "run.status.waiting_takeover": "En attente de ta prise en main",
    "run.status.completed": "Terminée",
    "run.status.failed": "A échoué",
    "run.status.cancelled": "Arrêtée",
    "run.trigger.user": "Depuis un message",
    "run.trigger.routine": "Depuis une routine",
    "run.trigger.webhook": "Depuis un webhook",
    "run.trigger.bot_message": "Depuis un autre bot",
    "run.trigger.tickets": "Travail sur un ticket",
    "run.trigger.created": "À la création",
    "run.trigger.helper_result": "Résultat d'un assistant",
    "run.trigger.retry": "Nouvelle tentative",
  },
  he: {
    "app.name": "Boma Bot",
    "shell.nav.threads": "שיחות",
    "shell.nav.activity": "פעילות",
    "shell.nav.primary": "ניווט ראשי",
    "shell.placeholder": "בחר בוט בעמודה השמאלית כדי לפתוח את השיחה שלו.",
    "shell.rail.bots": "בוטים",
    "shell.rail.activity": "פעילות",
    "shell.rail.primary": "מרחב עבודה",
    "shell.rail.bots.empty": "אין עדיין בוט.",
    "shell.rail.snoozed": "מושהה",
    "shell.rail.unsnooze": "בטל השעיה",
    "thread.snooze": "השעה את השיחה הזו",
    "thread.unsnooze": "בטל את ההשעיה",
    "landing.title": "שותפי AI קבועים, על חומרה שבבעלותך.",
    "landing.body":
      "גרסה זו כוללת את מעטפת היישום בלבד. השיחה, ציר ההרצה ולוח המחשב יגיעו אבן דרך אחר אבן דרך; ה-API שאליו הם מתחברים כבר מוגדר.",
    "landing.cta": "פתיחת מעטפת היישום",
    "notfound.title": "הדף לא נמצא",
    "notfound.body": "הכתובת הזאת אינה מתאימה לשום מסך. אולי הוא עבר, אולי הוא מעולם לא היה.",
    "notfound.back": "חזרה להתחלה",
    "common.retry": "נסה שוב",
    "thread.back": "חזרה לשיחות",
    "thread.reply.banner": "תשובה ל",
    "thread.reply.clear": "ביטול התשובה",
    "thread.reply.button": "תשובה",
    "thread.reactions.add": "הוסף תגובה",
    "thread.reactions.toggle": "הגב עם",
    "thread.reactions.picker": "בחר אמוג'י",
    "thread.card.approve": "אישור",
    "thread.card.answer": "תשובה",
    "thread.transcript": "שיחה",
    "thread.bot.active": "פעיל",
    "thread.bot.archived": "בארכיון",
    "thread.role.user": "את/ה",
    "thread.role.bot": "בוט",
    "thread.role.system": "מערכת",
    "thread.role.tool": "כלי",
    "thread.unread.label": "הודעות שלא נקראו",
    "thread.block.image": "תמונה",
    "thread.block.file": "קובץ",
    "thread.block.tool": "קריאה לכלי",
    "thread.block.card": "כרטיס",
    "thread.tool.running": "רץ",
    "thread.tool.finished": "הסתיים",
    "thread.loading": "טוען את השיחה",
    "thread.error.load": "לא ניתן היה לטעון את השיחה.",
    "thread.error.send": "ההודעה שלך לא נשלחה.",
    "thread.empty.intro": "אני מוכן. שאל אותי משהו ואני מתחיל לעבוד.",
    "thread.empty.suggestions": "שאלות מוצעות",
    "thread.empty.suggestion.1": "תכנן את השבוע שלי",
    "thread.empty.suggestion.2": "סכם את ההערות שלי",
    "thread.composer.label": "הודעה",
    "thread.composer.placeholder": "כתוב הודעה",
    "thread.composer.send": "שליחה",
    "thread.composer.sending": "שולח",
    "thread.offline": "לא בזמן אמת: ההודעות עדיין נשלחות.",
    "thread.live": "בזמן אמת",
    "thread.run.label": "הרצה",
    "activity.empty": "אין עדיין הרצות.",
    "activity.loading": "טוען את הפעילות",
    "activity.error": "לא ניתן היה לטעון את הפעילות.",
    "run.status.queued": "ממתין להתחלה",
    "run.status.leased": "נלכבד על ידי סוכן",
    "run.status.running": "עובד",
    "run.status.waiting_input": "ממתין לתשובה שלך",
    "run.status.waiting_takeover": "ממתין שתיקח את השליטה",
    "run.status.completed": "הושלם",
    "run.status.failed": "נכשל",
    "run.status.cancelled": "עצר",
    "run.trigger.user": "מהודעה",
    "run.trigger.routine": "משגרה",
    "run.trigger.webhook": "מוובהוק",
    "run.trigger.bot_message": "מבוט אחר",
    "run.trigger.tickets": "עבודה על כרטיס",
    "run.trigger.created": "בעת היצירה",
    "run.trigger.helper_result": "תוצאת עוזר",
    "run.trigger.retry": "ניסיון חוזר",
  },
};

const RTL_LANGUAGES = new Set(["ar", "fa", "he", "ur"]);

/** The language subtag of a BCP 47 tag, lowercased: `he-IL` becomes `he`. */
export function languageOf(locale: string): string {
  return locale.split("-")[0]?.toLowerCase() ?? "";
}

/** Whether a locale is written right to left. */
export function direction(locale: string): Direction {
  return RTL_LANGUAGES.has(languageOf(locale)) ? "rtl" : "ltr";
}

/** Whether this build has a catalogue for the locale. */
export function isSupported(locale: string): locale is Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(locale);
}

/**
 * Looks a message up, falling back to English and then to the key itself.
 *
 * Returning the key (rather than an empty string) is visible in a screenshot and in a test;
 * an empty string is not.
 */
export function t(locale: string, key: string): string {
  const language = languageOf(locale);
  if (isSupported(language)) {
    const translated = CATALOGUES[language][key];
    if (translated !== undefined) {
      return translated;
    }
  }
  return CATALOGUES.en[key] ?? key;
}
