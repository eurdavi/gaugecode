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
    windows: "Which limits to show",
    windowsHint: "Unticked limits disappear from the tray, the notch and the popup.",
    refresh: "Refresh",
    refreshEvery: "Check every",
    refreshHint:
      "60 seconds is what the vendors' own clients use. Faster is allowed but risks a rate limit, which makes the app wait longer than it saved.",
    seconds: (count: number) => `${count}s`,
    minutes: (count: number) => `${count} min`,
    notch: "Notch",
    notchVisible: "Show the notch overlay",
    notchEdge: "Screen edge",
    notchHint:
      "Folded it is click-through and stays inside the work area, so it never covers the taskbar or the Dock. Point at it to peek; click to keep it open.",
    notchUnsupported:
      "This session is Wayland, where an application cannot place its own window — the notch would drift, so it is turned off. The tray and the popup work normally. Log in to an X11 session to use it.",
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
  onboarding: {
    skip: "Skip",
    back: "Back",
    next: "Next",
    done: "Start using it",
    step: (current: number, total: number) => `${current} of ${total}`,
    welcomeTitle: "GaugeCode watches your limits",
    welcomeBody:
      "It shows how much of each AI tool's limit you have spent, and when each window resets. It reads only what those tools already left on this machine, and it never signs you in.",
    providersTitle: "Pick the tools you use",
    providersBody:
      "Turn on the ones you actually use. A tool you are not signed in to will simply say so instead of showing a number.",
    surfacesTitle: "Two places to look",
    surfacesBody:
      "The tray icon is a ring that fills with your usage — hover it for the figure, click it for the details. The notch is a thin sliver on a screen edge; point at it to expand it, click to keep it open.",
    honestTitle: "It will never invent a number",
    honestBody:
      "These usage endpoints are not documented by the vendors and can change without notice. When one breaks, GaugeCode says stale, needs sign-in, or error — never a plausible-looking guess.",
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
    glm: [
      "GLM has no CLI of its own, so the quota rides on a Z.ai Coding Plan key another tool holds.",
      "Set that key up in Claude Code (`settings.json`), ZCode or OpenCode.",
      "GaugeCode looks in all of those, in that order.",
    ],
    grok: [
      "Install the Grok CLI and run `grok login`.",
      "That writes `~/.grok/auth.json`, which GaugeCode reads.",
      "Only tokens issued by xAI are used — a corporate sign-in is never sent to the public endpoint.",
    ],
    opencode: [
      "Open OpenCode and run `opencode auth login`, then connect Go.",
      "That stores the `opencode-go` key GaugeCode reads.",
      "Only the Go plan is metered; Zen pay-as-you-go credit has no endpoint.",
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
    windows: "Quais limites mostrar",
    windowsHint: "Limites desmarcados desaparecem da bandeja, do notch e do popup.",
    refresh: "Atualização",
    refreshEvery: "Verificar a cada",
    refreshHint:
      "60 segundos é a cadência que os próprios clientes dos fornecedores usam. Mais rápido é permitido, mas arrisca um limite de taxa — e aí o app espera mais do que economizou.",
    seconds: (count: number) => `${count}s`,
    minutes: (count: number) => `${count} min`,
    notch: "Notch",
    notchVisible: "Mostrar o notch",
    notchEdge: "Borda da tela",
    notchHint:
      "Dobrado, ele deixa o clique passar e fica dentro da área útil, então nunca cobre a barra de tarefas nem o Dock. Aponte o mouse para espiar; clique para deixar aberto.",
    notchUnsupported:
      "Esta sessão é Wayland, onde um aplicativo não pode posicionar a própria janela — o notch ficaria à deriva, então está desligado. A bandeja e o popup funcionam normalmente. Entre numa sessão X11 para usá-lo.",
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
  onboarding: {
    skip: "Pular",
    back: "Voltar",
    next: "Avançar",
    done: "Começar a usar",
    step: (current: number, total: number) => `${current} de ${total}`,
    welcomeTitle: "O GaugeCode vigia seus limites",
    welcomeBody:
      "Ele mostra quanto do limite de cada ferramenta de IA você já gastou, e quando cada janela reseta. Só lê o que essas ferramentas já deixaram nesta máquina, e nunca faz login por você.",
    providersTitle: "Escolha as ferramentas que você usa",
    providersBody:
      "Ligue as que você usa de verdade. Uma ferramenta em que você não está logado simplesmente vai dizer isso, em vez de mostrar um número.",
    surfacesTitle: "Dois lugares para olhar",
    surfacesBody:
      "O ícone da bandeja é um anel que preenche com o seu uso — passe o mouse para ver o número, clique para os detalhes. O notch é uma tira fina na borda da tela; aponte o mouse para expandir, clique para deixar aberto.",
    honestTitle: "Ele nunca vai inventar um número",
    honestBody:
      "Esses endpoints de uso não são documentados pelos fornecedores e podem mudar sem aviso. Quando um quebrar, o GaugeCode diz desatualizado, precisa de login, ou erro — nunca um palpite com cara de verdade.",
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
    glm: [
      "O GLM não tem CLI próprio, então a cota anda numa chave do Z.ai Coding Plan que outra ferramenta guarda.",
      "Configure essa chave no Claude Code (`settings.json`), no ZCode ou no OpenCode.",
      "O GaugeCode procura em todos esses lugares, nessa ordem.",
    ],
    grok: [
      "Instale o Grok CLI e rode `grok login`.",
      "Isso grava o `~/.grok/auth.json`, que o GaugeCode lê.",
      "Só tokens emitidos pela xAI são usados — login corporativo nunca é enviado ao endpoint público.",
    ],
    opencode: [
      "Abra o OpenCode, rode `opencode auth login` e conecte o Go.",
      "Isso guarda a chave `opencode-go` que o GaugeCode lê.",
      "Só o plano Go é medido; crédito pay-as-you-go do Zen não tem endpoint.",
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
    windows: "Qué límites mostrar",
    windowsHint: "Los límites desmarcados desaparecen de la bandeja, del notch y del popup.",
    refresh: "Actualización",
    refreshEvery: "Comprobar cada",
    refreshHint:
      "60 segundos es la cadencia que usan los propios clientes de los proveedores. Más rápido está permitido, pero arriesga un límite de tasa — y entonces la app espera más de lo que ahorró.",
    seconds: (count: number) => `${count}s`,
    minutes: (count: number) => `${count} min`,
    notch: "Notch",
    notchVisible: "Mostrar el notch",
    notchEdge: "Borde de la pantalla",
    notchHint:
      "Plegado deja pasar los clics y se queda dentro del área de trabajo, así que nunca tapa la barra de tareas ni el Dock. Apunta con el ratón para asomarlo; haz clic para dejarlo abierto.",
    notchUnsupported:
      "Esta sesión es Wayland, donde una aplicación no puede colocar su propia ventana — el notch quedaría a la deriva, así que está desactivado. La bandeja y el popup funcionan con normalidad. Inicia una sesión X11 para usarlo.",
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
  onboarding: {
    skip: "Omitir",
    back: "Atrás",
    next: "Siguiente",
    done: "Empezar a usarlo",
    step: (current: number, total: number) => `${current} de ${total}`,
    welcomeTitle: "GaugeCode vigila tus límites",
    welcomeBody:
      "Muestra cuánto del límite de cada herramienta de IA has gastado, y cuándo se reinicia cada ventana. Solo lee lo que esas herramientas ya dejaron en este equipo, y nunca inicia sesión por ti.",
    providersTitle: "Elige las herramientas que usas",
    providersBody:
      "Activa las que realmente usas. Una herramienta en la que no has iniciado sesión simplemente lo dirá, en lugar de mostrar un número.",
    surfacesTitle: "Dos sitios donde mirar",
    surfacesBody:
      "El icono de la bandeja es un anillo que se llena con tu uso — pasa el ratón para ver la cifra, haz clic para los detalles. El notch es una tira fina en un borde de la pantalla; apunta con el ratón para expandirlo, haz clic para dejarlo abierto.",
    honestTitle: "Nunca se inventará un número",
    honestBody:
      "Estos endpoints de uso no están documentados por los proveedores y pueden cambiar sin avisar. Cuando uno se rompa, GaugeCode dirá desactualizado, requiere inicio de sesión, o error — nunca una suposición con aspecto creíble.",
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
    glm: [
      "GLM no tiene CLI propio, así que la cuota va en una clave del Z.ai Coding Plan que otra herramienta guarda.",
      "Configura esa clave en Claude Code (`settings.json`), en ZCode o en OpenCode.",
      "GaugeCode busca en todos esos sitios, en ese orden.",
    ],
    grok: [
      "Instala el CLI de Grok y ejecuta `grok login`.",
      "Eso escribe `~/.grok/auth.json`, que GaugeCode lee.",
      "Solo se usan tokens emitidos por xAI — un inicio de sesión corporativo nunca se envía al endpoint público.",
    ],
    opencode: [
      "Abre OpenCode, ejecuta `opencode auth login` y conecta Go.",
      "Eso guarda la clave `opencode-go` que GaugeCode lee.",
      "Solo se mide el plan Go; el crédito pay-as-you-go de Zen no tiene endpoint.",
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
