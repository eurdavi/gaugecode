<div align="center">

# GaugeCode

**Quanto do seu limite de IA você já gastou — sem abrir o painel de ninguém.**

Um app de bandeja para Windows, macOS e Linux que mostra, em tempo real, o consumo
das suas ferramentas de IA e quando cada limite reseta.

[![ci](https://github.com/eurdavi/gaugecode/actions/workflows/ci.yml/badge.svg)](https://github.com/eurdavi/gaugecode/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/eurdavi/gaugecode?include_prereleases&label=download)](https://github.com/eurdavi/gaugecode/releases)
[![licença MIT](https://img.shields.io/badge/licen%C3%A7a-MIT-blue)](LICENSE)

Português · [English](docs/README.en.md)

</div>

---

## O problema

Você usa Claude Code, Cursor e mais uma ou duas ferramentas de IA ao mesmo tempo.
Nenhuma delas te mostra o consumo fora do próprio produto. Resultado: você fica cego
para o limite até bater na parede no meio de uma tarefa.

O GaugeCode resolve isso com duas superfícies que ficam sempre à mão.

## Como aparece

<!--
Prints ficam em docs/screenshots/. Ainda não estão aqui — para gerar:
rode o app com GAUGECODE_DEMO=1 (dados de exemplo, nenhuma conta real exposta)
e salve as imagens com estes nomes.
-->

| | |
|---|---|
| <img src="docs/screenshots/tray.png" alt="Ícone da bandeja e popup de detalhes" width="420"> | **Bandeja.** O ícone é um anel que preenche com o seu uso e muda de cor: verde abaixo de 50 %, âmbar até 80 %, vermelho acima. Passe o mouse para o número exato, clique para o detalhe de cada janela de limite. |
| <img src="docs/screenshots/notch.png" alt="Notch expandido na borda da tela" width="420"> | **Notch.** Uma tira fina colada na borda da tela, sempre por cima. Dobrada, ela deixa o clique passar — não atrapalha nada. Aponte o mouse para expandir; clique para deixar aberta. |
| <img src="docs/screenshots/settings.png" alt="Tela de preferências" width="420"> | **Preferências.** Escolha quais ferramentas acompanhar, quais limites de cada uma aparecem, a borda e a animação do notch, o intervalo de checagem e o idioma. |

## Ferramentas suportadas

| Ferramenta | O que você faz | Onde o GaugeCode lê |
|---|---|---|
| **Claude Code** | rode `claude` e depois `/login` | `~/.claude/.credentials.json` |
| **Cursor** | apenas esteja logado no Cursor | o banco local do editor, em modo somente leitura |
| **Codex** | rode `codex` e faça login | `~/.codex/auth.json` |
| **GLM** (Z.ai) | configure a chave do Coding Plan no Claude Code, ZCode ou OpenCode | a chave que essa ferramenta já guarda |
| **Grok** | rode `grok login` | `~/.grok/auth.json` |
| **OpenCode** | rode `opencode auth login` e conecte o Go | a chave `opencode-go` |

### Não existe token para colar

Esse é o desenho do app, não uma limitação. Cada ferramenta já grava uma credencial
na sua máquina quando **você** faz login nela. O GaugeCode lê esse arquivo e consulta
o endpoint de uso do próprio fornecedor. Faça login na ferramenta e o número aparece
em menos de um minuto — as Preferências mostram o passo a passo de cada uma, e mostram
sozinhas quando alguma precisa de login.

## Instalação

Baixe o instalador em [Releases](https://github.com/eurdavi/gaugecode/releases).

- **Windows** — `.exe` (NSIS) ou `.msi`. O app não é assinado, então o SmartScreen avisa
  uma vez: clique em "Mais informações" e depois em "Executar assim mesmo".
- **macOS** — `.dmg` universal (Apple Silicon e Intel). Ainda sem notarização, então o
  Gatekeeper também avisa na primeira abertura.
- **Linux** — `.AppImage` ou `.deb`. O notch precisa de uma sessão **X11**: no Wayland
  um aplicativo não pode posicionar a própria janela, então lá ele fica desligado e a
  bandeja assume. Tudo o mais funciona igual.

Idiomas: português, inglês e espanhol. Ele segue o idioma do seu sistema.

## A ressalva honesta

Os endpoints de uso que este app lê **não são documentados** pelos fornecedores e podem
mudar sem aviso. Quando um quebrar, o GaugeCode vai dizer *desatualizado* (com a idade
do dado), *precisa de login* ou *erro*.

Ele **nunca** vai mostrar um percentual inventado. Esse é o único compromisso que não
se negocia aqui — um número errado é pior que número nenhum.

## O que o app lê, e o que ele nunca faz

- **Somente leitura.** Ele lê a credencial que cada ferramenta já deixou na sua máquina
  e chama o endpoint daquele fornecedor. Nunca escreve nesses arquivos, e o SQLite do
  Cursor é aberto em modo somente leitura.
- **Não faz login por você**, não troca de conta e não manda ping para "manter sessão viva".
- **Zero telemetria.** O único tráfego de rede é o app falando com o fornecedor. A
  integração contínua derruba o build se aparecer chamada de rede ou leitura de credencial
  fora da pasta dos adapters.
- **Token nunca vai para log.** Os logs têm status HTTP, nome do provider e duração. Nada além.
- **Atualização nunca se instala sozinha.** A checagem só avisa que existe versão nova;
  instalar é um clique seu.

## Contribuindo

Issues e pull requests são bem-vindos. Se você tem conta em alguma ferramenta que o app
ainda não cobre bem — especialmente **Codex, GLM, Grok ou OpenCode**, que eu não pude
testar com conta real — um relato do que apareceu na sua tela vale muito.

Para rodar local: [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/),
Node LTS e `pnpm`.

```sh
pnpm install
pnpm tauri dev
```

`GAUGECODE_DEMO=1` roda com dados de exemplo, sem rede e sem ler credencial — útil para
gravar vídeo ou tirar print sem expor conta.

## Licença

MIT — veja [LICENSE](LICENSE).
