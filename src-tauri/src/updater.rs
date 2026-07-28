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
fn endpoint(app: &AppHandle) -> Option<tauri::Url> {
    let base = server::load(app).url;
    tauri::Url::parse(&format!(
        "{base}/api/v1/desktop/update/{{{{target}}}}-{{{{arch}}}}/{{{{current_version}}}}"
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
                "A atualização não pôde ser concluída. Você segue na versão atual — tente de novo mais tarde.",
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
    /// O `{{target}}-{{arch}}` tem de sobreviver ao `Url::parse` — que
    /// percent-encoda `{` e `}` no path. O plugin substitui as duas formas
    /// (literal e encodada), então o que este teste protege é a FORMA do path:
    /// um `/` a mais entre target e arch faria a rota do ShvIA receber
    /// `darwin/aarch64` e devolver 404 em vez do manifesto.
    #[test]
    fn endpoint_tem_target_e_arch_no_mesmo_segmento() {
        let base = "https://ai.shvia.org";
        let url = tauri::Url::parse(&format!(
            "{base}/api/v1/desktop/update/{{{{target}}}}-{{{{arch}}}}/{{{{current_version}}}}"
        ))
        .expect("url válida");

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
}
