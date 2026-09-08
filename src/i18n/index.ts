/**
 * Translations for the webview windows. The tray menu and the tooltip are drawn
 * before any window exists, so those live in `src-tauri/src/i18n.rs` — if you
 * add a string there, add it here too.
 *
 * Messages are plain nested objects rather than dotted key strings, so the
 * compiler catches a missing or misspelled translation: `Messages` is derived
 * from the English catalogue and the other two must satisfy it.
 */

import type { Language, ProviderId } from "../types/usage";

const en = {
  app: {
    name: "GaugeCode",
    loading: "loading…",
    demo: "demo data",
    refresh: "Refresh now",
    settings: "Settings",
    close: "Close",
    version: (version: string) => `version ${version}`,
  },
  status: {
    waiting: "waiting for first reading",
    fresh: "up to date",
    old: (age: string) => `${age} old`,
    needsAuth: "needs sign-in",
    off: "off",
    notImplemented: "not in this build yet",
    noReading: "no reading",
  },
  window: {
    resetsIn: (duration: string) => `resets in ${duration}`,
    resettingNow: "resetting now",
    underAMinute: "under a minute",
  },
  fidelity: {
    official: "from the vendor",
    derived: "estimated",
    manual: "entered by hand",
  },
  notch: {
    pin: "Click to keep open",
    unpin: "Click to unpin",
    noProvider: "no provider enabled",
  },
  settings: {
    readOnly:
      "Read-only. GaugeCode reads the credential each tool already left on this machine and asks that vendor's own usage endpoint. It never signs you in and never sends anything anywhere else.",
    providers: "Providers",
    tray: "tray",
    trayHint: "Shown on the tray icon",
    enable: (name: string) => `Enable ${name}`,
    manage: "manage",
    connect: "How to connect",
    notch: "Notch",
    notchVisible: "Show the notch overlay",
    notchEdge: "Screen edge",
    notchHint:
      "Folded it is click-through and stays inside the work area, so it never covers the taskbar or the Dock. Point at it to peek; click to keep it open.",
    animation: "Animation",
    appearance: "Appearance",
    language: "Language",
    languageSystem: "System",
    system: "System",
    autostart: "Start with the system",
    autoUpdate: "Check for updates automatically",
    updateAvailable: (version: string) => `Version ${version} is available`,
    installUpdate: "Install and restart",
    checkNow: "Check now",
    upToDate: "You are on the latest version",
    updates: "Updates",
    howItWorks: "How this works / what the app reads",
  },
  edge: {
    top: "Top",
    bottom: "Bottom",
    left: "Left",
    right: "Right",
  },
  animation: {
    slide: "Slide",
    fade: "Fade",
    instant: "None",
  },
  language: {
    en: "English",
    "pt-BR": "Português (Brasil)",
    es: "Español",
  },
  /** Answers "where do I put the token?" — nowhere; you sign in to the tool. */
  connect: {
    intro:
      "There is no token to paste. Each tool writes a credential on this machine when you sign in to it, and GaugeCode reads that file. Sign in to the tool and the number appears within a minute.",
    claude: [
      "Open a terminal and run `claude`.",
      "Type `/login` and finish the sign-in in the browser it opens.",
      "That writes your subscription login into the file GaugeCode reads.",
    ],
    cursor: [
      "Open Cursor and sign in, if you are not already.",
      "Nothing else to do: GaugeCode reads Cursor's own session from its local database, read-only.",
    ],
    codex: [
      "Install the Codex CLI (`npm i -g @openai/codex`).",
      "Run `codex` and sign in with your ChatGPT account.",
      "That writes `~/.codex/auth.json`, which GaugeCode reads.",
    ],
  },
};

/**
 * Shape every catalogue has to match, derived from the English one. Deliberately
 * without `as const`: the literal types would then be the English strings
 * themselves and no translation could satisfy them.
 */
export type Messages = typeof en;

const ptBR: Messages = {
  app: {
    name: "GaugeCode",
    loading: "carregando…",
    demo: "dados de demonstração",
    refresh: "Atualizar agora",
    settings: "Preferências",
    close: "Fechar",
    version: (version: string) => `versão ${version}`,
  },
  status: {
    waiting: "aguardando a primeira leitura",
    fresh: "em dia",
    old: (age: string) => `há ${age}`,
    needsAuth: "precisa de login",
    off: "desligado",
    notImplemented: "ainda não está nesta versão",
    noReading: "sem leitura",
  },
  window: {
    resetsIn: (duration: string) => `reseta em ${duration}`,
    resettingNow: "resetando agora",
    underAMinute: "menos de um minuto",
  },
  fidelity: {
    official: "direto do fornecedor",
    derived: "estimado",
    manual: "informado à mão",
  },
  notch: {
    pin: "Clique para fixar aberto",
    unpin: "Clique para soltar",
    noProvider: "nenhum provider ligado",
  },
  settings: {
    readOnly:
      "Somente leitura. O GaugeCode lê a credencial que cada ferramenta já deixou nesta máquina e consulta o endpoint de uso do próprio fornecedor. Ele nunca faz login por você e nunca manda nada para outro lugar.",
    providers: "Providers",
    tray: "bandeja",
    trayHint: "Aparece no ícone da bandeja",
    enable: (name: string) => `Ativar ${name}`,
    manage: "gerenciar",
    connect: "Como conectar",
    notch: "Notch",
    notchVisible: "Mostrar o notch",
    notchEdge: "Borda da tela",
    notchHint:
      "Dobrado, ele deixa o clique passar e fica dentro da área útil, então nunca cobre a barra de tarefas nem o Dock. Aponte o mouse para espiar; clique para deixar aberto.",
    animation: "Animação",
    appearance: "Aparência",
    language: "Idioma",
    languageSystem: "Do sistema",
    system: "Sistema",
    autostart: "Iniciar com o sistema",
    autoUpdate: "Procurar atualizações automaticamente",
    updateAvailable: (version: string) => `A versão ${version} está disponível`,
    installUpdate: "Instalar e reiniciar",
    checkNow: "Procurar agora",
    upToDate: "Você está na versão mais recente",
    updates: "Atualizações",
    howItWorks: "Como isso funciona / o que o app lê",
  },
  edge: {
    top: "Topo",
    bottom: "Base",
    left: "Esquerda",
    right: "Direita",
  },
  animation: {
    slide: "Deslizar",
    fade: "Esmaecer",
    instant: "Nenhuma",
  },
  language: {
    en: "English",
    "pt-BR": "Português (Brasil)",
    es: "Español",
  },
  connect: {
    intro:
      "Não existe token para colar. Cada ferramenta grava uma credencial nesta máquina quando você faz login nela, e o GaugeCode lê esse arquivo. Faça login na ferramenta e o número aparece em menos de um minuto.",
    claude: [
      "Abra um terminal e rode `claude`.",
      "Digite `/login` e conclua o login no navegador que abrir.",
      "Isso grava o login da sua assinatura no arquivo que o GaugeCode lê.",
    ],
    cursor: [
      "Abra o Cursor e faça login, se ainda não fez.",
      "Nada além disso: o GaugeCode lê a sessão do próprio Cursor no banco local dele, em modo somente leitura.",
    ],
    codex: [
      "Instale o Codex CLI (`npm i -g @openai/codex`).",
      "Rode `codex` e faça login com sua conta do ChatGPT.",
      "Isso grava o `~/.codex/auth.json`, que o GaugeCode lê.",
    ],
  },
};

const es: Messages = {
  app: {
    name: "GaugeCode",
    loading: "cargando…",
    demo: "datos de demostración",
    refresh: "Actualizar ahora",
    settings: "Preferencias",
    close: "Cerrar",
    version: (version: string) => `versión ${version}`,
  },
  status: {
    waiting: "esperando la primera lectura",
    fresh: "al día",
    old: (age: string) => `hace ${age}`,
    needsAuth: "requiere inicio de sesión",
    off: "apagado",
    notImplemented: "todavía no está en esta versión",
    noReading: "sin lectura",
  },
  window: {
    resetsIn: (duration: string) => `se reinicia en ${duration}`,
    resettingNow: "reiniciándose ahora",
    underAMinute: "menos de un minuto",
  },
  fidelity: {
    official: "directo del proveedor",
    derived: "estimado",
    manual: "introducido a mano",
  },
  notch: {
    pin: "Clic para mantener abierto",
    unpin: "Clic para soltar",
    noProvider: "ningún proveedor activo",
  },
  settings: {
    readOnly:
      "Solo lectura. GaugeCode lee la credencial que cada herramienta ya dejó en este equipo y consulta el endpoint de uso del propio proveedor. Nunca inicia sesión por ti y nunca envía nada a ningún otro sitio.",
    providers: "Proveedores",
    tray: "bandeja",
    trayHint: "Se muestra en el icono de la bandeja",
    enable: (name: string) => `Activar ${name}`,
    manage: "gestionar",
    connect: "Cómo conectar",
    notch: "Notch",
    notchVisible: "Mostrar el notch",
    notchEdge: "Borde de la pantalla",
    notchHint:
      "Plegado deja pasar los clics y se queda dentro del área de trabajo, así que nunca tapa la barra de tareas ni el Dock. Apunta con el ratón para asomarlo; haz clic para dejarlo abierto.",
    animation: "Animación",
    appearance: "Apariencia",
    language: "Idioma",
    languageSystem: "Del sistema",
    system: "Sistema",
    autostart: "Iniciar con el sistema",
    autoUpdate: "Buscar actualizaciones automáticamente",
    updateAvailable: (version: string) => `La versión ${version} está disponible`,
    installUpdate: "Instalar y reiniciar",
    checkNow: "Buscar ahora",
    upToDate: "Estás en la última versión",
    updates: "Actualizaciones",
    howItWorks: "Cómo funciona / qué lee la app",
  },
  edge: {
    top: "Arriba",
    bottom: "Abajo",
    left: "Izquierda",
    right: "Derecha",
  },
  animation: {
    slide: "Deslizar",
    fade: "Desvanecer",
    instant: "Ninguna",
  },
  language: {
    en: "English",
    "pt-BR": "Português (Brasil)",
    es: "Español",
  },
  connect: {
    intro:
      "No hay ningún token que pegar. Cada herramienta escribe una credencial en este equipo cuando inicias sesión en ella, y GaugeCode lee ese archivo. Inicia sesión en la herramienta y el número aparece en menos de un minuto.",
    claude: [
      "Abre una terminal y ejecuta `claude`.",
      "Escribe `/login` y completa el inicio de sesión en el navegador que se abra.",
      "Eso escribe el login de tu suscripción en el archivo que GaugeCode lee.",
    ],
    cursor: [
      "Abre Cursor e inicia sesión, si aún no lo has hecho.",
      "Nada más: GaugeCode lee la sesión de Cursor desde su base de datos local, en modo solo lectura.",
    ],
    codex: [
      "Instala el CLI de Codex (`npm i -g @openai/codex`).",
      "Ejecuta `codex` e inicia sesión con tu cuenta de ChatGPT.",
      "Eso escribe `~/.codex/auth.json`, que GaugeCode lee.",
    ],
  },
};

const CATALOGUES: Record<Language, Messages> = { en, "pt-BR": ptBR, es };

export function messagesFor(language: Language): Messages {
  return CATALOGUES[language] ?? en;
}

export const LANGUAGES: Language[] = ["en", "pt-BR", "es"];

/** Sign-in steps for one provider, in the current language. */
export function connectSteps(messages: Messages, provider: ProviderId): readonly string[] {
  return messages.connect[provider];
}
