//! Auto-update do desktop (item **D1**; ADR-022) — o **ShvIA serve o manifesto**.
//!
//! O par minisign existe desde 28/07/2026 e a pública está no `tauri.conf.json`;
//! era a única peça que faltava (ver ADR-020 e `SHVIA-WEB/docs/INFRA/AUTO-UPDATE-DESKTOP.md`).
//!
//! ## Tudo aqui é API RUST — nenhuma capability para a página remota
//!
//! A capability `default` **não** declara `updater:default`, e é decisão, não
//! esquecimento: o ADR-001 diz que a página do servidor não recebe comando nativo, e
//! updater é o pior candidato possível a exceção — um servidor comprometido que
//! consegue chamar `install` escolhe qual binário roda na máquina do usuário. O
//! plugin é dirigido daqui, como o `dialog` e o `notification` já são.
//!
//! ## O endpoint segue o servidor CONFIGURADO, não o embutido
//!
//! `plugins.updater.endpoints` no `tauri.conf.json` é **estático**, e desde o D4
//! (ADR-019) o servidor é configurável — on-prem existe. Um endpoint fixo faria a
//! instalação on-prem consultar o `ai.shvia.org` e instalar o build de *outra*
//! infraestrutura. Então o endpoint é remontado em runtime a partir de
//! `server::load()`, e o do `tauri.conf.json` vale só como default.
//!
//! ## AVISA e pergunta — não instala sozinho
//!
//! Decisão do Samir (28/07/2026): update silencioso que reinicia o app no meio de
//! uma conversa com a Anna é pior que o problema que resolve. Uma pergunta só, com o
//! reinício dito na própria pergunta — e não duas (baixar? reiniciar?), porque no
//! Windows o instalador toma a mão do processo e a segunda pergunta seria mentira.
//!
//! ## Fail-open, e silencioso quando automático
//!
//! Sem rede, 204, JSON estranho, assinatura inválida: **no-op**. A checagem
//! automática nunca abre diálogo de erro — quem não pediu para checar não deve
//! receber um alerta sobre isso. A checagem **manual** (menu Ajuda) sempre responde,
//! inclusive "você já está na mais recente": menu que não dá sinal parece quebrado.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

use crate::server;

/// Espera antes da PRIMEIRA checagem. O boot já disputa rede com o probe do
/// servidor e o carregamento da página remota; checar update no meio disso rouba
/// banda de quem está esperando a tela aparecer.
const ATRASO_INICIAL: Duration = Duration::from_secs(20);

/// Intervalo das checagens seguintes. A janela desta casca fica aberta por dias
/// (ADR-011), então "só no boot" na prática nunca checaria.
const INTERVALO: Duration = Duration::from_secs(6 * 60 * 60);

/// Guarda anti-reentrância: duas checagens simultâneas baixariam ~80 MB duas vezes
/// e, pior, poderiam chamar `install` concorrente sobre o mesmo bundle. Acontece de
/// verdade — o timer periódico e o clique no menu são independentes.
static EM_ANDAMENTO: AtomicBool = AtomicBool::new(false);

/// Onde fica a versão que o usuário dispensou (ao lado do `server.json`).
const CONFIG_FILE: &str = "updater.json";

/// Teto do texto de release notes no diálogo. O `notes` vem do servidor e pode ser
/// um changelog inteiro; diálogo nativo não rola, então um texto longo empurra os
/// botões para fora da tela em vez de informar.
const MAX_NOTAS: usize = 280;

/// Marcador que o pacote Arch instala para se identificar (ADR-028).
///
/// É um contrato com `packaging/arch/PKGBUILD` — mudar o caminho ou o conteúdo
/// aqui exige mudar lá junto, e o efeito de esquecer é silencioso: o app volta a
/// tentar o auto-update que não funciona.
#[cfg(target_os = "linux")]
const MARCADOR_PACMAN: &str = "/usr/share/shvia-desktop/instalado-por";

/// Esta instalação veio do pacote do Arch?
///
/// ## Por que um arquivo em vez de perguntar ao plugin
///
/// O `tauri-plugin-updater` não tem como responder isso: `bundle_type()` lê um
/// marcador que o BUNDLER grava no binário, e os valores possíveis são só
/// Deb/Rpm/AppImage/Msi/Nsis — não existe variante pacman. Como o pacote Arch é
/// remontado a partir do payload do `.deb`, o binário chega aqui se dizendo DEB.
///
/// O que aconteceria sem esta checagem (verificado no fonte do plugin 2.10.1):
/// `install_inner` despacharia para `install_deb`, que roda `pkexec dpkg -i` —
/// e `dpkg` não existe no Arch. O usuário baixaria ~80 MB para receber um erro.
/// Com dpkg vindo do AUR seria pior: arquivos Debian num sistema pacman, sem o
/// banco de pacotes saber.
///
/// Ler um arquivo custa um `stat` a cada 6 h; consultar `pacman -Qo` custaria um
/// processo. O arquivo também é a resposta certa quando o pacman nem está no PATH.
#[cfg(target_os = "linux")]
fn instalado_por_pacman() -> bool {
    marcador_diz_pacman(std::path::Path::new(MARCADOR_PACMAN))
}

#[cfg(not(target_os = "linux"))]
fn instalado_por_pacman() -> bool {
    false
}

/// A leitura em si, separada do caminho fixo para o teste exercitar ESTA função e
/// não uma reimplementação dela (mesmo motivo do `endpoint_para`).
///
/// Ausente, ilegível ou com outro conteúdo = não é pacman. Fail-open de propósito:
/// errar para "tenta atualizar" mantém o comportamento de hoje em toda instalação
/// que não é do pacote Arch; errar para "não atualiza" desligaria o auto-update de
/// quem depende dele, e em silêncio.
///
/// `cfg(test)` junto do `cfg(linux)` porque fora do Linux ninguém a chama em build
/// normal — sem isso, o build do macOS/Windows acusa dead_code por uma função que
/// existe de propósito.
#[cfg(any(target_os = "linux", test))]
fn marcador_diz_pacman(caminho: &std::path::Path) -> bool {
    std::fs::read_to_string(caminho)
        .map(|s| s.trim() == "pacman")
        .unwrap_or(false)
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(CONFIG_FILE))
}

/// Versão que o usuário mandou esperar, se houver.
fn versao_dispensada(app: &AppHandle) -> Option<String> {
    config_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| {
            v.get("dispensada")
                .and_then(|x| x.as_str())
                .map(str::to_string)
        })
}

/// Grava a dispensa. Por VERSÃO e não booleano: dispensar a 1.1.0 não pode calar o
/// aviso da 1.2.0 — senão um "Depois" clicado uma vez desliga o updater para sempre,
/// que é a falha silenciosa que este item existe para não ter.
fn dispensar(app: &AppHandle, versao: &str) {
    let Some(path) = config_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::json!({ "dispensada": versao }).to_string();
    let _ = std::fs::write(path, json);
}

/// Endpoint do manifesto no servidor configurado.
///
/// O `{{target}}-{{arch}}` monta o `darwin-aarch64` que a rota do ShvIA espera
/// (ela faz `explode('-')` no primeiro segmento); `{{current_version}}` é o que
/// deixa o servidor responder `204` em vez de oferecer downgrade para um build
/// local mais novo que o publicado.
///
/// ## `?bundle=` — e por que na QUERY, não no path
///
/// `{{bundle_type}}` vira `deb`, `rpm`, `appimage`, `msi` ou `nsis`: o formato pelo
/// qual ESTA instalação foi feita. O plugin precisa dele porque despacha a
/// instalação por aí (`install_inner` → `install_deb`/`install_rpm`/
/// `install_appimage`) — quem instalou o `.deb` e recebe um AppImage quebra depois
/// de baixar tudo. No macOS e no Windows não muda nada na prática; no Linux é a
/// diferença entre atualizar e não.
///
/// Vai na query e **não** como segmento novo do path porque as instalações ≤ 1.1.0
/// já existem e continuam pedindo a rota antiga: um path novo as deixaria tomando
/// 404 (erro no log, sem nada a fazer) em vez do 204 que elas sabem tratar. Query
/// desconhecida é ignorada pelo servidor antigo, e o novo trata a ausência como
/// AppImage. O plugin substitui o placeholder na query igual ao path.
fn endpoint(app: &AppHandle) -> Option<tauri::Url> {
    endpoint_para(&server::load(app).url)
}

/// A montagem em si, separada do `AppHandle` para o teste exercitar ESTA string e
/// não uma cópia dela. Antes o teste remontava o formato à mão — e um teste que
/// duplica o que verifica passa verde enquanto o endpoint real está errado.
fn endpoint_para(base: &str) -> Option<tauri::Url> {
    tauri::Url::parse(&format!(
        "{base}/api/v1/desktop/update/{{{{target}}}}-{{{{arch}}}}/{{{{current_version}}}}?bundle={{{{bundle_type}}}}"
    ))
    .ok()
}

fn notificar(app: &AppHandle, titulo: &str, corpo: &str) {
    let _ = app
        .notification()
        .builder()
        .title(titulo)
        .body(corpo)
        .show();
}

/// Diálogo nativo. **Só pode ser chamado fora da thread principal** —
/// `blocking_show` na main thread trava o app (é o próprio loop de eventos que
/// precisa girar para o diálogo responder). Todos os chamadores daqui vêm de
/// `std::thread::spawn`.
fn perguntar(app: &AppHandle, titulo: &str, texto: &str, sim: &str, nao: &str) -> bool {
    app.dialog()
        .message(texto)
        .title(titulo)
        .buttons(MessageDialogButtons::OkCancelCustom(
            sim.to_string(),
            nao.to_string(),
        ))
        .blocking_show()
}

fn avisar(app: &AppHandle, titulo: &str, texto: &str, kind: MessageDialogKind) {
    let _ = app
        .dialog()
        .message(texto)
        .title(titulo)
        .kind(kind)
        .blocking_show();
}

/// Traduz a falha de instalação para algo que dê para AGIR.
///
/// Existe por causa de um caso real (28/07/2026, máquina Linux): o diálogo dizia só
/// "não pôde ser concluída — tente de novo mais tarde", e "mais tarde" nunca ia
/// resolver, porque a causa era estrutural. Diagnosticar exigiu rodar o app pelo
/// terminal para ler o `eprintln`. Mensagem de erro que não distingue "tente de novo"
/// de "isto nunca vai funcionar assim" custa uma sessão de investigação.
fn explicar_falha(e: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;

    match e {
        // O pacote baixado não é do formato que ESTA instalação sabe instalar. No
        // Linux é o caso comum: o plugin instala conforme o bundle em execução, e
        // cliente ≤ 1.1.0 não informa o formato ao servidor (o default é AppImage).
        E::InvalidUpdaterFormat => {
            "O pacote recebido não é do formato desta instalação.\n\n\
             No Linux isso acontece quando o app foi instalado por .deb/.rpm mas o \
             servidor entregou o AppImage. Instale esta versão à mão UMA vez — a \
             partir dela o app informa o próprio formato e o problema não volta."
                .to_string()
        }
        // Chegou a rodar dpkg/rpm e não completou: quase sempre é a elevação
        // (pkexec/sudo) negada ou ausente.
        E::PackageInstallFailed => {
            "O instalador do sistema não completou.\n\n\
             Atualizar um install por .deb/.rpm precisa de senha de administrador \
             (pkexec/sudo) — se o pedido não apareceu ou foi cancelado, é isso. O \
             AppImage atualiza sem precisar de senha."
                .to_string()
        }
        outro => format!("Motivo: {outro}"),
    }
}

/// Uma passada completa: checa, pergunta, baixa, instala, reinicia.
///
/// `manual` = veio do menu Ajuda. Muda duas coisas: passa por cima da versão
/// dispensada (quem clicou "verificar" quer verificar) e responde na tela mesmo
/// quando não há nada a fazer.
async fn executar(app: &AppHandle, manual: bool) {
    let Some(url) = endpoint(app) else {
        if manual {
            avisar(
                app,
                "Atualização",
                "Não deu para montar o endereço de atualização a partir do servidor configurado.",
                MessageDialogKind::Warning,
            );
        }
        return;
    };

    let updater = match app.updater_builder().endpoints(vec![url]) {
        Ok(b) => match b.build() {
            Ok(u) => u,
            Err(e) => {
                // Chega aqui quando a `pubkey` do `tauri.conf.json` não parseia.
                // É erro de configuração do build, não do ambiente do usuário.
                eprintln!("[updater] não deu para construir: {e}");
                if manual {
                    avisar(
                        app,
                        "Atualização",
                        "A verificação de atualizações não está configurada neste build.",
                        MessageDialogKind::Warning,
                    );
                }
                return;
            }
        },
        Err(e) => {
            eprintln!("[updater] endpoint inválido: {e}");
            return;
        }
    };

    let atual = app.package_info().version.to_string();

    let update = match updater.check().await {
        Ok(Some(u)) => u,
        // 204 do servidor (em dia, plataforma não empacotada, artefato sem
        // assinatura, recurso desligado) cai todo aqui — e é o caminho comum.
        Ok(None) => {
            if manual {
                avisar(
                    app,
                    "Atualização",
                    &format!("Você já está na versão mais recente ({atual})."),
                    MessageDialogKind::Info,
                );
            }
            return;
        }
        Err(e) => {
            eprintln!("[updater] falha ao checar: {e}");
            if manual {
                avisar(
                    app,
                    "Atualização",
                    "Não deu para verificar agora. Confira a conexão com o servidor e tente de novo.",
                    MessageDialogKind::Warning,
                );
            }
            return;
        }
    };

    if !manual && versao_dispensada(app).as_deref() == Some(update.version.as_str()) {
        return;
    }

    let notas = update
        .body
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if s.chars().count() > MAX_NOTAS {
                let corte: String = s.chars().take(MAX_NOTAS).collect();
                format!("{corte}…")
            } else {
                s.to_string()
            }
        })
        .map(|s| format!("\n\n{s}"))
        .unwrap_or_default();

    // Instalação por pacman: avisa e PARA aqui (ADR-028). O gerenciador de pacotes
    // é dono dos arquivos em /usr, e o plugin não sabe atualizar um pacote dele —
    // oferecer "instalar agora" seria prometer o que só falharia depois de ~80 MB
    // de download. `dispensar` no fim para o ciclo automático não repetir o mesmo
    // aviso de 6 em 6 horas; a checagem manual passa por cima e sempre responde.
    if instalado_por_pacman() {
        avisar(
            app,
            "Atualização disponível",
            &format!(
                "O ShvIA Desktop {} está disponível (você tem a {}).{}\n\n\
                 Esta instalação veio do pacote do Arch, então quem atualiza é o \
                 pacman:\n\n    sudo pacman -Syu\n\n\
                 O app não pode se substituir sozinho em arquivos que o gerenciador \
                 de pacotes é dono.",
                update.version, atual, notas
            ),
            MessageDialogKind::Info,
        );
        dispensar(app, &update.version);
        return;
    }

    let texto = format!(
        "O ShvIA Desktop {} está disponível (você tem a {}).{}\n\nBaixar e instalar agora? O app vai reiniciar quando terminar.",
        update.version, atual, notas
    );

    if !perguntar(
        app,
        "Atualização disponível",
        &texto,
        "Instalar e reiniciar",
        "Depois",
    ) {
        dispensar(app, &update.version);
        return;
    }

    // O download é de ~80 MB e não há barra de progresso (o diálogo nativo não tem
    // uma, e uma janela nossa só para isso quebraria a casca fina). A notificação é
    // o único sinal de que algo começou — sem ela o app fica minutos calado depois
    // do clique e parece que o botão não fez nada.
    notificar(
        app,
        "Baixando a atualização",
        &format!("ShvIA Desktop {} — o app reinicia ao terminar.", update.version),
    );

    match update.download_and_install(|_, _| {}, || {}).await {
        Ok(()) => {
            // `restart` já trata ser chamado fora da main thread.
            app.restart();
        }
        Err(e) => {
            eprintln!("[updater] falha ao instalar: {e}");
            avisar(
                app,
                "Atualização",
                &format!(
                    "A atualização não pôde ser concluída. Você segue na versão atual.\n\n{}",
                    explicar_falha(&e)
                ),
                MessageDialogKind::Error,
            );
        }
    }
}

/// Envelope do `executar` com a guarda de reentrância.
async fn ciclo(app: AppHandle, manual: bool) {
    if EM_ANDAMENTO.swap(true, Ordering::SeqCst) {
        if manual {
            avisar(
                &app,
                "Atualização",
                "Já existe uma verificação de atualização em andamento.",
                MessageDialogKind::Info,
            );
        }
        return;
    }
    executar(&app, manual).await;
    EM_ANDAMENTO.store(false, Ordering::SeqCst);
}

/// Agenda a checagem automática: uma depois do boot, e a cada 6 h.
///
/// Thread própria (e não `async_runtime::spawn`) porque o ciclo **bloqueia** por
/// tempo indeterminado em diálogo nativo esperando o usuário; num worker do runtime
/// isso prenderia um slot compartilhado com o resto do app.
pub fn agendar(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(ATRASO_INICIAL);
        loop {
            tauri::async_runtime::block_on(ciclo(app.clone(), false));
            std::thread::sleep(INTERVALO);
        }
    });
}

/// Checagem manual (menu **Ajuda → Verificar atualizações…**).
pub fn verificar_agora(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        tauri::async_runtime::block_on(ciclo(app, true));
    });
}

#[cfg(test)]
mod tests {
    use super::{endpoint_para, marcador_diz_pacman};

    /// O marcador é o ÚNICO sinal de que o app não deve tentar se atualizar
    /// sozinho (ADR-028). Se a leitura afrouxar — aceitar arquivo vazio, casar por
    /// prefixo, ignorar o conteúdo — o guard passa a disparar em instalação que
    /// não é do pacman, e o auto-update do .deb/AppImage morre em silêncio.
    #[test]
    fn marcador_so_vale_com_o_conteudo_exato() {
        let dir = std::env::temp_dir().join("shvia-teste-marcador");
        std::fs::create_dir_all(&dir).expect("criar dir de teste");

        let caso = |nome: &str, conteudo: &str| -> bool {
            let p = dir.join(nome);
            std::fs::write(&p, conteudo).expect("escrever marcador");
            marcador_diz_pacman(&p)
        };

        assert!(caso("ok", "pacman"), "o conteúdo exato tem de valer");
        // O PKGBUILD escreve com heredoc, que deixa o \n no fim — se o trim sair,
        // o pacote real para de ser reconhecido e ninguém percebe até o update.
        assert!(caso("nl", "pacman\n"), "quebra de linha no fim tem de valer");

        assert!(!caso("vazio", ""), "arquivo vazio não é marcador");
        assert!(!caso("outro", "deb"), "outro conteúdo não é pacman");
        assert!(!caso("prefixo", "pacman-ish"), "casamento é exato, não por prefixo");
        assert!(
            !marcador_diz_pacman(&dir.join("nao-existe")),
            "arquivo ausente é o caso comum (todo install que não é do Arch)"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// O `{{target}}-{{arch}}` tem de sobreviver ao `Url::parse` — que
    /// percent-encoda `{` e `}` no path. O plugin substitui as duas formas
    /// (literal e encodada), então o que este teste protege é a FORMA do path:
    /// um `/` a mais entre target e arch faria a rota do ShvIA receber
    /// `darwin/aarch64` e devolver 404 em vez do manifesto.
    #[test]
    fn endpoint_tem_target_e_arch_no_mesmo_segmento() {
        let url = endpoint_para("https://ai.shvia.org").expect("url válida");

        let path = url.path();
        assert!(
            path.starts_with("/api/v1/desktop/update/"),
            "prefixo da rota mudou: {path}"
        );
        // Um único segmento para target+arch, separados por hífen.
        let cauda = path.trim_start_matches("/api/v1/desktop/update/");
        let segmentos: Vec<&str> = cauda.split('/').collect();
        assert_eq!(segmentos.len(), 2, "esperado <target-arch>/<versão>: {cauda}");
        assert!(
            segmentos[0].contains("target") && segmentos[0].contains("arch"),
            "target e arch precisam ficar no mesmo segmento: {}",
            segmentos[0]
        );
        assert!(
            segmentos[1].contains("current_version"),
            "a versão atual precisa ir no path (é o que habilita o 204): {}",
            segmentos[1]
        );
    }

    /// O `bundle` tem de ir na QUERY, e o placeholder tem de sobreviver literal ali.
    ///
    /// É o que faz o Linux atualizar: o plugin instala conforme o bundle do app em
    /// execução, então um install de `.deb` que receba AppImage quebra depois de
    /// baixar tudo. E na query — não no path — para as instalações ≤ 1.1.0, que
    /// pedem a rota antiga, continuarem recebendo 204 em vez de 404.
    #[test]
    fn endpoint_manda_o_bundle_na_query() {
        let url = endpoint_para("https://ai.shvia.org").expect("url válida");

        let query = url.query().unwrap_or_default();
        assert!(
            query.contains("bundle="),
            "o parâmetro bundle saiu da query: {query}"
        );
        assert!(
            query.contains("bundle_type"),
            "o placeholder {{{{bundle_type}}}} tem de chegar literal para o plugin \
             substituir; se ele foi encodado ou perdido, o servidor recebe texto \
             cru e cai no default: {query}"
        );
        // E não pode ter virado segmento de path — isso quebraria cliente antigo.
        assert!(
            !url.path().contains("bundle"),
            "bundle não pode ser segmento de path: {}",
            url.path()
        );
    }

    /// Servidor on-prem: a base vem da config do usuário (D4/ADR-019), então a
    /// montagem não pode assumir `ai.shvia.org` nem porta padrão.
    #[test]
    fn endpoint_respeita_base_on_prem_com_porta() {
        let url = endpoint_para("https://shvia.interno.cliente:8443").expect("url válida");

        assert_eq!(url.host_str(), Some("shvia.interno.cliente"));
        assert_eq!(url.port(), Some(8443));
        assert!(url.path().starts_with("/api/v1/desktop/update/"));
    }
}
