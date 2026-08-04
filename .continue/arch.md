Sim — agora os três build scripts geram pacote Arch (.pkg.tar.zst)
Rota implementada: conversão do .deb via fpm no próprio host Debian de build (web1), sem precisar de máquina Arch. Em todos, é best-effort: sem fpm no PATH, o script avisa como instalar e segue sem derrubar o build.

SSHVTERM-DESKTOP (1.2.45) — build-local.sh: bloco novo após o bundle do Tauri converte o .deb com deps mapeadas para o Arch (webkit2gtk-4.1, gtk3); sai em bundle/pacman/ e aparece na listagem final. Idempotente no reuso de build.

SHVIA-DESKTOP (1.1.14 → 1.1.15) — mesmo bloco no build-local.sh, com libayatana-appindicator a mais (o app usa tray-icon). Criei o CHANGELOG.md que faltava (o handoff que o COMMITTER espera).

GITHUB-DESKTOP (0.4.0 → 0.4.1) — buildPacman novo em script/package.ts registrado no fan-out best-effort do Linux (default agora deb,rpm,appimage,pacman, selecionável por LINUX_FORMATS); deps espelham o github-desktop-bin do AUR. tsc limpo.

Avisos importantes
Não testei o caminho Linux — estamos no macOS e esse código só roda em host Linux. No web1, antes do primeiro build:

sudo apt install ruby ruby-dev build-essential zstd libarchive-tools && sudo gem install fpm
O .pkg.tar.zst fica fora do --publish por enquanto, nos dois Tauri — motivos diferentes, anotados nos scripts:
sshvterm: o releases:add do site recusa a extensão (ALLOWED_EXTENSIONS no SSHVTERM-WEB) → follow-up lá;
shvia: o fpm não gera .sig e o endpoint trata artefato sem assinatura como inexistente — entraria no manifesto e quebraria a verificação da publicação.
Commits: sshvterm e shvia estão com árvore pronta para o COMMITTER (version.md + changelog). O GITHUB-DESKTOP não tem .committer.yml — deixei as mudanças sem commit; me avisa se quiser que eu commite (development).
Lembrete: o AppImage que os três já geram roda no Arch hoje, sem esperar nada disso.