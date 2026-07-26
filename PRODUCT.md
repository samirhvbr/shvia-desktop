# Product

> Sintetizado do [README.md](README.md), [.continue/escopo-projeto.md](.continue/escopo-projeto.md)
> e [docs/decisoes.md](docs/decisoes.md). Fonte estratégica para os comandos do
> Impeccable; se divergir dos docs acima, os docs vencem.

## Register

product

## Users

Equipe interna da Blue3 (uso profissional, jornada de trabalho inteira) usando o
ShvIA — a plataforma de IA da empresa (`https://ai.shvia.org`) — como app de
chat/trabalho: conversas com modelos, projetos/workspaces com arquivos (RAG),
comparação de modelos, administração. O desktop existe para dar janela própria,
ícone, atalhos e presença nativa (tray, notificações) ao mesmo produto web.

## Product Purpose

Cliente desktop multiplataforma (Tauri 2) do ShvIA, como **shell fino**: a
janela carrega o Blade remoto — "mesmas funções" é literal, zero UI reescrita.
O servidor é a fonte da verdade (dados, senhas, permissões); o cliente não tem
banco nem segredos. Sucesso = o usuário esquece que não está no navegador,
com as conveniências de um app nativo (janela própria, multi-janela,
deep-link `shvia://`, atualizações).

## Brand Personality

Sóbrio, técnico, confiável. Identidade ShvIA/Blue3 (azure #34B3EC sobre dark
frio; Space Grotesk para identidade, IBM Plex Sans/Mono para leitura/dados) —
definida em `SHVIA/public/css/tokens.css` (servidor, fonte única da verdade
visual) e espelhada em `brand/`. Dark-only. Nenhum branding de terceiros
(Claude/Anthropic) em artefato algum.

## Anti-references

- UI própria que compita ou divirja do ShvIA web — o desktop **ecoa** o design
  system do servidor, nunca inventa um paralelo.
- Chrome de app pesado (toolbars, painéis nativos redundantes) sobre uma UI web
  que já é completa.
- Estética "wrapper Electron genérico": splash demorado, telas de erro cruas,
  diálogos com cara de SO puro destoando do tema dark do produto.

## Design Principles

1. **A UI é o ShvIA** — superfícies nativas (menus, diálogos, telas locais)
   ecoam os tokens do servidor; consistência vence novidade.
2. **Menor privilégio** — nada nativo exposto à página remota (ADR-001);
   pontes injetadas são cirúrgicas, autocontidas e defensivas.
3. **Online-first honesto** — estados de falha (offline, servidor fora) têm
   tela/tarja com a marca, ação de retry e nunca dado obsoleto fingindo ser vivo.
4. **Discrição nativa** — o valor do desktop está em janela/atalhos/integração
   com o SO, não em decoração; toda adição de chrome precisa se pagar.

## Accessibility & Inclusion

Sem requisito formal de WCAG documentado; alvo prático: AA em contraste nas
superfícies locais/injetadas (o remoto herda o ShvIA), foco visível, fechamento
por Esc em diálogos, `prefers-reduced-motion` respeitado nas pontes injetadas.
Idioma do produto: português (pt-BR).
