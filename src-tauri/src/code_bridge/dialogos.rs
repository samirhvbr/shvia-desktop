//! The native dialogs the page asks for: pick a folder (the gesture that authorizes it,
//! cerca.rs), pick files (their bytes go back, the page has no disk), and save a generated file.
//! Each one remembers its last folder. Split out of `code_bridge.rs` in 1.6.41.

use super::*;

/// Teto do artefato salvo pela ponte (50 MB). A página é confiável (host
/// canônico), mas o base64 vem por `eval` de mensagem — um limite explícito
/// evita que uma resposta malformada tente materializar meio gigabyte na RAM.
pub(super) const MAX_SAVE_BYTES: usize = 50 * 1024 * 1024;

/// `saveFile` — salva um artefato GERADO pelo modelo onde o usuário ESCOLHER.
///
/// A página manda bytes (base64) + nome sugerido; nunca URL. O `/api/v1/files/
/// {id}` do ShvIA é autenticado por sessão, e a sessão vive na WebView — o Rust
/// não a tem. Com os bytes vindo prontos, o lado nativo só abre o diálogo e
/// escreve, sem precisar saber nada de autenticação.
///
/// Cancelar NÃO é erro: volta `{saved:false}` e o web fica quieto.
pub(super) fn save_file(window: &WebviewWindow, req: String, v: &serde_json::Value) {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;

    let nome = sanitize_filename(v.get("name").and_then(|x| x.as_str()).unwrap_or(""));
    let bytes = match v
        .get("dataBase64")
        .and_then(|x| x.as_str())
        .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s).ok())
    {
        Some(b) if !b.is_empty() && b.len() <= MAX_SAVE_BYTES => b,
        _ => {
            reply(window, &req, false, serde_json::json!({ "error": "conteudo invalido" }));
            return;
        }
    };

    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file().set_file_name(&nome);
    // Mesma dor do anexo: salvar dois artefatos seguidos obrigava a refazer o
    // caminho na segunda vez.
    if let Some(dir) = ultima_pasta(window, "salvar") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.save_file(move |path| {
        let Some(path) = path else {
            reply(&win, &req, true, serde_json::json!({ "saved": false }));
            return;
        };
        let resultado = path
            .into_path()
            .map_err(|e| e.to_string())
            .and_then(|p| std::fs::write(&p, &bytes).map(|_| p).map_err(|e| e.to_string()));
        match resultado {
            Ok(p) => {
                if let Some(pai) = p.parent() {
                    grava_ultima_pasta(&win, "salvar", pai);
                }
                reply(
                    &win,
                    &req,
                    true,
                    serde_json::json!({ "saved": true, "path": p.to_string_lossy() }),
                )
            }
            Err(e) => reply(&win, &req, false, serde_json::json!({ "error": e })),
        }
    });
}

/// Nome de arquivo vindo da PÁGINA: só o basename, sem separador de diretório
/// nem `..`. O diálogo já obriga o usuário a escolher a pasta, mas o campo do
/// nome não pode carregar caminho — nem no macOS, onde "/" é separador e ":"
/// tem herança de path.
pub(super) fn sanitize_filename(nome: &str) -> String {
    let base = nome
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('.')
        .replace(':', "-");
    let limpo: String = base
        .chars()
        .filter(|c| !c.is_control() && !matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        .take(120)
        .collect();

    if limpo.is_empty() {
        "arquivo".to_string()
    } else {
        limpo
    }
}

// ── última pasta usada nos diálogos ─────────────────────────────────────────
//
// O diálogo do SO não lembra onde você estava: cada abertura nasce nos
// favoritos/atalhos, e para anexar o arquivo vizinho do que você acabou de
// anexar era preciso refazer o caminho inteiro (queixa de 03/08/2026). Isso é
// estado do DISPOSITIVO, não do usuário nem do projeto — mora aqui, ao lado do
// `modo-code-bindings.json`, e não sobe para o servidor.
//
// Uma chave por PROPÓSITO ('arquivos', 'pasta', 'salvar'): a pasta de onde você
// anexa contexto raramente é a pasta onde você salva um artefato, e uma chave só
// faria os três se atrapalharem.
pub(super) const MAX_PICK_BYTES: usize = 10 * 1024 * 1024; // = MAX_FOLDER_FILE_BYTES do web

pub(super) const MAX_PICK_FILES: usize = 30;

pub(super) fn ultimas_pastas_path(window: &WebviewWindow) -> Option<PathBuf> {
    let dir = window.app_handle().path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("ultimas-pastas.json"))
}

pub(super) fn ultima_pasta(window: &WebviewWindow, chave: &str) -> Option<PathBuf> {
    let map: serde_json::Map<String, serde_json::Value> = ultimas_pastas_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let p = PathBuf::from(map.get(chave)?.as_str()?);
    // A pasta pode ter sido removida/desmontada desde a última vez. Apontar o
    // diálogo para um caminho morto é pior que não apontar: alguns backends
    // abrem vazios em vez de cair no default.
    p.is_dir().then_some(p)
}

pub(super) fn grava_ultima_pasta(window: &WebviewWindow, chave: &str, dir: &std::path::Path) {
    if !dir.is_dir() {
        return;
    }
    let mut map: serde_json::Map<String, serde_json::Value> = ultimas_pastas_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    map.insert(chave.to_string(), serde_json::json!(dir.to_string_lossy()));
    if let (Some(p), Ok(s)) = (ultimas_pastas_path(window), serde_json::to_string_pretty(&map)) {
        let _ = std::fs::write(p, s);
    }
}

pub(super) fn pick_folder(window: &WebviewWindow, req: String) {
    use tauri_plugin_dialog::DialogExt;
    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file();
    if let Some(dir) = ultima_pasta(window, "pasta") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.pick_folder(move |path| {
        let escolhido = path.and_then(|p| p.into_path().ok());
        // Guarda o PAI: quem escolheu ~/x/TDAH quase sempre volta para escolher
        // outro projeto em ~/x, não para entrar de novo no TDAH.
        if let Some(pai) = escolhido.as_ref().and_then(|p| p.parent()) {
            grava_ultima_pasta(&win, "pasta", pai);
        }
        // 🔴 É AQUI que a cerca do Modo Code cresce, e só aqui (F-12): escolher no diálogo
        // nativo é o gesto do usuário. Nenhuma mensagem da página autoriza pasta.
        if let Some(dir) = escolhido.as_ref() {
            autorizar_pasta(&win, dir);
        }
        let data = serde_json::json!({
            "path": escolhido.map(|p| p.to_string_lossy().to_string()),
        });
        reply(&win, &req, true, data);
    });
}

/// Seletor de arquivos NATIVO para os anexos do chat (arquivos do projeto).
///
/// Existe porque o `<input type="file">` da WebView não deixa escolher a pasta
/// inicial — é decisão do navegador, e no WebKitGTK ela cai nos favoritos toda
/// vez. Aqui o diálogo é nosso, então abre onde você estava.
///
/// Devolve os BYTES em base64, não os caminhos: a página é quem tem a sessão
/// autenticada e faz o upload, e ela não enxerga o disco. Mesmo desenho do
/// `save_file`, na direção contrária.
///
/// Arquivo acima do teto do servidor (10 MB) sai em `skipped` em vez de derrubar
/// a seleção inteira — o resto do que foi escolhido continua valendo.
pub(super) fn pick_files(window: &WebviewWindow, req: String) {
    use base64::Engine;
    use tauri_plugin_dialog::DialogExt;

    let win = window.clone();
    let mut dialogo = window.app_handle().dialog().file();
    if let Some(dir) = ultima_pasta(window, "arquivos") {
        dialogo = dialogo.set_directory(dir);
    }
    dialogo.pick_files(move |paths| {
        let Some(paths) = paths else {
            reply(&win, &req, true, serde_json::json!({ "files": [], "canceled": true }));
            return;
        };

        let mut arquivos = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        let mut pasta_lembrada = false;

        for fp in paths.into_iter().take(MAX_PICK_FILES) {
            let Ok(path) = fp.into_path() else { continue };
            let nome = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if nome.is_empty() {
                continue;
            }
            // A pasta vem do PRIMEIRO arquivo que deu certo — é onde o usuário
            // estava quando confirmou.
            if !pasta_lembrada {
                if let Some(pai) = path.parent() {
                    grava_ultima_pasta(&win, "arquivos", pai);
                    pasta_lembrada = true;
                }
            }
            match ler_escolhido(&path, MAX_PICK_BYTES) {
                Ok(bytes) => arquivos.push(serde_json::json!({
                    "name": nome,
                    "size": bytes.len(),
                    "dataBase64": base64::engine::general_purpose::STANDARD.encode(&bytes),
                })),
                Err(motivo) => skipped.push(format!("{nome}: {motivo}")),
            }
        }

        reply(
            &win,
            &req,
            true,
            serde_json::json!({ "files": arquivos, "skipped": skipped }),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    /// O nome vem da PÁGINA. O diálogo escolhe a pasta; o campo do nome não pode
    /// reintroduzir caminho por cima dela.
    #[test]
    fn nome_de_arquivo_nunca_carrega_caminho() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/tmp/imagem.png"), "imagem.png");
        assert_eq!(sanitize_filename(r"C:\Windows\x.png"), "x.png");
        // macOS: ":" tem herança de separador de path no Finder.
        assert_eq!(sanitize_filename("pasta:arquivo.png"), "pasta-arquivo.png");
    }

    #[test]
    fn nome_vazio_ou_so_pontos_vira_fallback() {
        assert_eq!(sanitize_filename(""), "arquivo");
        assert_eq!(sanitize_filename("   "), "arquivo");
        assert_eq!(sanitize_filename("..."), "arquivo");
        assert_eq!(sanitize_filename("/"), "arquivo");
    }

    #[test]
    fn nome_normal_passa_intacto() {
        assert_eq!(sanitize_filename("imagem-gerada-1.png"), "imagem-gerada-1.png");
        assert_eq!(sanitize_filename("shvia-1785032029.svg"), "shvia-1785032029.svg");
    }

    #[test]
    fn caracteres_de_controle_e_curinga_saem() {
        assert_eq!(sanitize_filename("a\nb*c?.png"), "abc.png");
        assert!(sanitize_filename(&"x".repeat(500)).len() <= 120);
    }
}
