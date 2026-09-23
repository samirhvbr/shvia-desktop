//! Ícone de bandeja / menubar + "iniciar com o sistema" (item **D2**; ADR-024).
//!
//! ## O buraco que isto fecha
//!
//! O [ADR-011] entregou notificação nativa para os alertas de preço e resolveu o caso
//! do meio: **janela aberta, mas em segundo plano**. Ficou de fora o caso de baixo —
//! **janela fechada**. Sem janela o app saía, e sem app não há notificação: o alerta só
//! chegava por Telegram, que é justamente a dependência que o ADR-011 queria dispensar.
//!
//! Um alerta de preço que só existe com a janela aberta na frente do usuário não é
//! alerta — é badge. Três peças fecham isso, e nenhuma sozinha:
//!
//! 1. **A bandeja** — o app existir sem janela precisa de um lugar onde ele apareça.
//!    Processo rodando sem ícone nenhum é processo que o usuário mata no gerenciador de
//!    tarefas achando que é vírus.
//! 2. **Fechar recolhe em vez de sair** — é o que mantém o processo vivo para receber o
//!    alerta.
//! 3. **Iniciar com o sistema** — é o que faz o app estar de pé depois do boot, quando
//!    ninguém lembrou de abri-lo.
//!
//! ## Fechar ≠ sair, e por que isso vale o incômodo
//!
//! No macOS fechar a janela **já** não encerra o app (o ícone segue no Dock, e o
//! `RunEvent::Reopen` recria a janela no clique). No Windows e no Linux, fechar mata. O
//! resultado antes deste item era a pior combinação possível: o mesmo produto entregava
//! alerta com a janela fechada num SO e não nos outros, **em silêncio** — nenhum erro,
//! nenhuma tela, só a notificação que não chega.
//!
//! Então unificamos: fechar recolhe nos três. E o incômodo conhecido de "app que não
//! morre" é respondido de três jeitos, porque um só não bastaria:
//!
//! - **`Sair` na bandeja e no menu** `Arquivo` — a saída de verdade continua a um clique;
//! - **aviso nativo na primeira vez**, uma única vez na vida da instalação: sem ele, a
//!   janela desaparece e o usuário conclui que o app travou;
//! - **a preferência é desligável** ali mesmo na bandeja, onde a confusão acontece.
//!
//! ## O estado é LIDO, não lembrado
//!
//! O menu é reconstruído a cada mudança, consultando o estado real (`is_enabled()` do
//! autostart, o host configurado do D4). Guardar cópias dos itens e atualizá-las
//! deixaria a bandeja mentir no dia em que algo mudasse por fora — desinstalar o
//! LaunchAgent na mão, por exemplo. Bandeja que mostra estado errado é pior que bandeja
//! sem estado: a primeira faz o usuário confiar.
//!
//! [ADR-011]: ../../docs/decisoes.md

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

/// Id do ícone, para recuperá-lo com `app.tray_by_id` e trocar o menu.
pub const TRAY_ID: &str = "shvia-tray";

const PREFS_FILE: &str = "tray.json";

/// Preferências da bandeja.
///
/// Arquivo separado do `server.json` de propósito: aquele é o **perímetro** (host
/// interno da navegação), e um JSON corrompido lá é tela branca. Misturar preferência
/// cosmética no mesmo arquivo aumentaria as chances de mexer no que não pode quebrar.
#[derive(Clone, Copy, Debug)]
pub struct Prefs {
    /// Fechar a última janela recolhe para a bandeja em vez de sair.
    pub close_to_tray: bool,
    /// O aviso de "continuo rodando aqui" já foi mostrado alguma vez?
    pub avisou: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        // `close_to_tray` liga por default, e não é agressividade: sem ele o item D2 não
        // entrega nada — o alerta de preço continua não chegando com a janela fechada,
        // que é o buraco inteiro. E é o comportamento que o macOS já tinha; o default
        // faz os outros dois SOs pararem de divergir.
        Self { close_to_tray: true, avisou: false }
    }
}

fn prefs_path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join(PREFS_FILE))
}

/// Lê as preferências. **Nunca falha**: arquivo ausente, ilegível ou com lixo dentro
/// cai no default. Preferência de bandeja não pode impedir o app de subir.
pub fn ler(app: &AppHandle) -> Prefs {
    let padrao = Prefs::default();
    let Some(v) = prefs_path(app)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    else {
        return padrao;
    };

    Prefs::do_json(&v)
}

impl Prefs {
    /// Núcleo PURO da leitura, separado para poder ser testado sem app rodando.
    ///
    /// Campo ausente cai no default — e o default de `close_to_tray` é `true`. Um
    /// `unwrap_or(false)` aqui desligaria o item D2 inteiro em silêncio: o app voltaria
    /// a sair no fechamento, o alerta de preço pararia de chegar, e não haveria erro
    /// nenhum em lugar nenhum. É o tipo de bug que só aparece em reclamação de usuário
    /// meses depois, então tem teste.
    fn do_json(v: &serde_json::Value) -> Self {
        let padrao = Self::default();
        Self {
            close_to_tray: v
                .get("close_to_tray")
                .and_then(|x| x.as_bool())
                .unwrap_or(padrao.close_to_tray),
            avisou: v.get("avisou").and_then(|x| x.as_bool()).unwrap_or(false),
        }
    }
}

fn gravar(app: &AppHandle, p: Prefs) {
    let Some(path) = prefs_path(app) else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let json = serde_json::json!({ "close_to_tray": p.close_to_tray, "avisou": p.avisou });
    // Falha de escrita é silenciosa de propósito: a preferência volta ao default no
    // próximo boot, e um diálogo de erro para isso interromperia o usuário por nada.
    let _ = std::fs::write(path, json.to_string());
}

/// Desliga o "fechar mantém rodando".
///
/// Chamado pelo `setup` quando a criação da bandeja FALHA. Sem ícone e com a preferência
/// ligada, fechar a janela esconderia o app sem volta — nem janela, nem ícone. É a
/// combinação que o item D7 reporta, e aqui ela é evitada antes de acontecer.
pub fn desligar_recolher(app: &AppHandle) {
    let mut p = ler(app);
    if p.close_to_tray {
        p.close_to_tray = false;
        gravar(app, p);
    }
}

/// Monta o menu da bandeja **lendo o estado real** a cada chamada.
fn montar_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let prefs = ler(app);

    // O host configurado (item D4) é o estado que mais importa mostrar: numa instalação
    // on-prem, "para qual servidor este app está apontando?" é a primeira pergunta de
    // qualquer suporte, e hoje só o modal "Sobre" responde.
    let host = crate::server::configured_host().unwrap_or_else(|| crate::SERVER_HOST.to_string());
    let rotulo_host = MenuItem::with_id(app, "tray-host", format!("Servidor: {host}"), false, None::<&str>)?;

    let abrir = MenuItem::with_id(app, "tray-open", "Abrir o ShvIA", true, None::<&str>)?;
    let atualizar = MenuItem::with_id(app, "tray-update", "Verificar atualizações…", true, None::<&str>)?;

    // `is_enabled()` do plugin, não uma cópia nossa: se o usuário apagar o LaunchAgent
    // na mão, a bandeja tem de contar a verdade.
    let autostart_ligado = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(
        app,
        "tray-autostart",
        "Iniciar com o sistema",
        true,
        autostart_ligado,
        None::<&str>,
    )?;

    let recolher = CheckMenuItem::with_id(
        app,
        "tray-close-to-tray",
        "Fechar mantém rodando aqui",
        true,
        prefs.close_to_tray,
        None::<&str>,
    )?;

    // On GTK the predefined `quit` is dropped silently (see the app menu in lib.rs), so
    // until 1.6.9 the Linux tray had no way to quit. A plain item handled below instead.
    #[cfg(target_os = "linux")]
    let sair = MenuItem::with_id(app, "tray-quit", "Sair do ShvIA", true, None::<&str>)?;
    // The predefined item ends through Tauri's own route, which `ExitRequested` reads as a
    // user request (`code` set) rather than "the last window closed".
    #[cfg(not(target_os = "linux"))]
    let sair = PredefinedMenuItem::quit(app, Some("Sair do ShvIA"))?;
    Menu::with_items(
        app,
        &[
            &rotulo_host,
            &PredefinedMenuItem::separator(app)?,
            &abrir,
            &atualizar,
            &PredefinedMenuItem::separator(app)?,
            &autostart,
            &recolher,
            &PredefinedMenuItem::separator(app)?,
            &sair,
        ],
    )
}

/// Cria o ícone de bandeja. Chamado uma vez, no `setup`.
pub fn instalar(app: &AppHandle) -> tauri::Result<()> {
    let menu = montar_menu(app)?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .tooltip("ShvIA")
        .on_menu_event(|app, event| tratar_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            // Clique esquerdo mostra a janela — convenção de Windows e Linux. No macOS
            // o clique abre o menu (é o que a barra de menus faz), então lá esta
            // condição nunca é satisfeita e não há conflito.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                mostrar(tray.app_handle());
            }
        });

    // macOS (04/08, pedido do Samir): a barra de menus usa o TEMPLATE "AI" —
    // sem o logo da Blue3, "fica mais óbvio do que se trata". Preto + alpha com
    // `icon_as_template`, então o sistema o inverte conforme o tema (era a
    // dívida de asset anotada aqui desde a v1). Windows/Linux seguem com o
    // ícone colorido do app: tray colorido é a convenção nesses sistemas.
    #[cfg(target_os = "macos")]
    {
        let ai = tauri::image::Image::from_bytes(include_bytes!(
            "../icons/tray-ai-template.png"
        ))?;
        builder = builder.icon(ai).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    // Menu no clique esquerdo só no macOS; nos outros o esquerdo mostra a janela e o
    // direito abre o menu.
    builder = builder.show_menu_on_left_click(cfg!(target_os = "macos"));

    builder.build(app)?;
    Ok(())
}

fn tratar_menu(app: &AppHandle, id: &str) {
    match id {
        "tray-open" => mostrar(app),
        // Linux only: `exit` runs `RunEvent::Exit` (sidecars and pending login killed).
        "tray-quit" => app.exit(0),
        "tray-update" => crate::updater::verificar_agora(app),
        "tray-autostart" => {
            let ligado = app.autolaunch().is_enabled().unwrap_or(false);
            let r = if ligado {
                app.autolaunch().disable()
            } else {
                app.autolaunch().enable()
            };
            if let Err(e) = r {
                // Falha real e comum: política de MDM no Windows, ou diretório de
                // LaunchAgents sem permissão. Avisar é obrigatório — o menu voltaria
                // ao estado antigo na reconstrução e pareceria que o clique não pegou.
                let _ = app
                    .notification()
                    .builder()
                    .title("ShvIA")
                    .body(format!("Não foi possível mudar o início automático: {e}"))
                    .show();
            }
            recarregar_menu(app);
        }
        "tray-close-to-tray" => {
            let mut p = ler(app);
            p.close_to_tray = !p.close_to_tray;
            gravar(app, p);
            recarregar_menu(app);
        }
        _ => {}
    }
}

/// Reconstrói o menu a partir do estado real.
fn recarregar_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), montar_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Traz o app de volta: mostra e foca a janela, recriando-a se não existir mais.
pub fn mostrar(app: &AppHandle) {
    let janelas = app.webview_windows();

    if let Some(win) = janelas.get("main").or_else(|| janelas.values().next()) {
        let _ = win.show();
        // `unminimize` antes do foco: uma janela minimizada aceita `set_focus` sem
        // aparecer, e o clique na bandeja pareceria não fazer nada.
        let _ = win.unminimize();
        let _ = win.set_focus();
        return;
    }

    // Sem janela nenhuma (recolhida com `close`, que destrói): recria.
    crate::rebuild_main_window(app);
}

/// Chamado quando a última janela foi fechada e vamos manter o app vivo.
///
/// O aviso sai **uma vez na vida da instalação**. Repetido, viraria ruído que o usuário
/// aprende a ignorar — e ele precisa ser lido exatamente na primeira vez, que é quando a
/// janela some e a conclusão natural é "o app travou".
pub fn avisar_uma_vez(app: &AppHandle) {
    let mut p = ler(app);
    if p.avisou {
        return;
    }
    p.avisou = true;
    gravar(app, p);

    let onde = if cfg!(target_os = "macos") {
        "na barra de menus, no alto da tela"
    } else {
        "na área de notificação, ao lado do relógio"
    };

    let _ = app
        .notification()
        .builder()
        .title("O ShvIA continua rodando")
        .body(format!(
            "Fechei a janela, mas sigo {onde} para avisar de alertas de preço. \
             Clique no ícone para reabrir, ou use \"Sair do ShvIA\" para encerrar de vez."
        ))
        .show();
}

#[cfg(test)]
mod tests {
    use super::Prefs;

    fn do_str(json: &str) -> Prefs {
        Prefs::do_json(&serde_json::from_str(json).expect("json de teste válido"))
    }

    /// O default é o item D2 funcionando. Se um dia alguém trocar por `false`, o app
    /// volta a sair no fechamento e o alerta de preço para de chegar — sem erro, sem
    /// tela, sem log. O buraco do ADR-011 reabriria em silêncio.
    #[test]
    fn recolher_para_a_bandeja_e_o_default() {
        assert!(Prefs::default().close_to_tray);
        assert!(!Prefs::default().avisou, "o aviso não pode nascer como já dado");
    }

    /// Arquivo sem o campo = instalação que nunca abriu a bandeja. Tem de herdar o
    /// default, não o `false` do tipo.
    #[test]
    fn campo_ausente_herda_o_default() {
        assert!(do_str("{}").close_to_tray);
        assert!(do_str(r#"{"avisou":true}"#).close_to_tray);
    }

    /// E quem DESLIGOU de propósito continua desligado. É a outra metade: um default
    /// que ignorasse o `false` explícito seria uma preferência que não obedece.
    #[test]
    fn desligar_explicitamente_e_respeitado() {
        assert!(!do_str(r#"{"close_to_tray":false}"#).close_to_tray);
    }

    /// Tipo errado no arquivo (editado à mão, ou versão futura do formato) não pode
    /// virar `false` acidental — `as_bool()` devolve `None` e cai no default.
    #[test]
    fn tipo_errado_cai_no_default_em_vez_de_desligar() {
        assert!(do_str(r#"{"close_to_tray":"sim"}"#).close_to_tray);
        assert!(do_str(r#"{"close_to_tray":0}"#).close_to_tray);
        assert!(!do_str(r#"{"avisou":"talvez"}"#).avisou);
    }

    /// O aviso é uma vez na VIDA da instalação: gravado `true`, tem de ser lido `true`.
    /// Se voltasse `false`, a notificação "continuo rodando aqui" apareceria a cada
    /// fechamento — e ruído repetido é ruído que o usuário aprende a ignorar.
    #[test]
    fn o_aviso_dado_permanece_dado() {
        assert!(do_str(r#"{"close_to_tray":true,"avisou":true}"#).avisou);
    }
}
