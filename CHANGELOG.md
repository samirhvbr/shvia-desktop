# Changelog

Entradas no formato da mensagem de commit (`versão - comentário`, AGENTS.md),
mais recente primeiro. É daqui que a skill COMMITTER tira a mensagem (AGENTS.md §PS).

## 1.1.15 - build-local.sh (Linux) ganha o pacote Arch (.pkg.tar.zst) via fpm

- O bundler do Tauri 2.x só gera deb/rpm/AppImage no Linux; o pacote pacman agora
  sai convertendo o .deb com o fpm na própria máquina Debian (sem host Arch).
  Deps mapeadas para os nomes do Arch (webkit2gtk-4.1, gtk3, libayatana-appindicator
  — o app usa tray-icon). Best-effort: sem fpm no PATH, avisa como instalar e segue.
- Fora do release.json/publish por enquanto: o fpm não gera o .sig do updater e o
  endpoint do ShvIA trata artefato sem assinatura como inexistente — entrar no
  manifesto quebraria a verificação da publicação. Distribuição manual por ora.
