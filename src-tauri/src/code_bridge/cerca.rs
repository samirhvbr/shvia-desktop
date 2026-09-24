//! The Code mode fence (F-12, ADR-031): the folders the PERSON picked in a native dialog, and
//! nothing else. Every arm of `handle_message` that reads the disk or starts a process asks
//! `pasta_autorizada` first. Also the device-local project→folder bindings. Split out of
//! `code_bridge.rs` in 1.6.41.

use super::*;

//
// 🔴 **Achado F-12 da revisão de 01/09/2026.** As ações de arquivo do bridge
// (`listTree`, `readFile`, `gitStatus`, `gitDiff`, `spawn`) confinavam o alvo dentro de um
// `path` que a **própria página** mandava na mensagem. `read_file` exigia que o arquivo
// estivesse dentro do `path` — e o `path` vinha de quem estava pedindo, então
// `readFile('/', '/etc/passwd')` passava, e `listTree('/')` também.
//
// O token de capacidade fecha **iframe**, não a página: um XSS no SHVIA-WEB (cuja CSP nasce
// desligada) ou um asset de terceiro comprometido roda NA origem que tem o token. O próprio
// `spawn` reconhece esse ator e por isso valida a `url` — a mesma mensagem, no mesmo handler,
// lia qualquer arquivo do disco e devolvia o conteúdo em `_reply`.
//
// A cerca passa a vir de um **gesto**: só o diálogo nativo (`pick_folder`) autoriza uma
// pasta. A página pode pedir o que quiser; se não estiver na lista, não abre.
//
// ⚠️ **Migração, e o que ela custa:** instalações existentes já têm pastas vinculadas em
// `modo-code-bindings.json`, escrito pela página em resposta a um `pickFolder` real. Sem
// semear a lista com elas, todo mundo perderia o vínculo e teria de escolher a pasta de
// novo. A semeadura acontece **uma vez**, e o que ela herda é o que já estava lá antes desta
// versão — daí em diante, só o diálogo acrescenta. Isso é aceitar um resíduo estreito (uma
// instalação já comprometida antes desta versão continua com o que gravou) em troca de não
// quebrar quem nunca foi atacado.
pub(super) fn pastas_autorizadas_path(window: &WebviewWindow) -> Option<PathBuf> {
    let dir = window.app_handle().path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("pastas-autorizadas.json"))
}

/// Lista de pastas autorizadas, semeada uma única vez a partir dos vínculos antigos.
pub(super) fn pastas_autorizadas(window: &WebviewWindow) -> Vec<PathBuf> {
    let Some(p) = pastas_autorizadas_path(window) else {
        return Vec::new();
    };
    if let Ok(s) = std::fs::read_to_string(&p) {
        if let Ok(v) = serde_json::from_str::<Vec<String>>(&s) {
            return v.into_iter().map(PathBuf::from).collect();
        }
    }
    // Primeira execução desta versão: herda os vínculos que a página já tinha gravado.
    let semente: Vec<String> = load_bindings(window)
        .values()
        .filter_map(|x| x.as_str())
        .filter(|s| !s.is_empty() && PathBuf::from(s).is_dir())
        .map(String::from)
        .collect();
    if let Ok(s) = serde_json::to_string_pretty(&semente) {
        let _ = std::fs::write(&p, s);
    }
    semente.into_iter().map(PathBuf::from).collect()
}

/// Acrescenta uma pasta à cerca. **Só o diálogo nativo chama isto** — é o gesto.
pub(super) fn autorizar_pasta(window: &WebviewWindow, dir: &std::path::Path) {
    let Ok(canon) = std::fs::canonicalize(dir) else {
        return;
    };
    let mut lista = pastas_autorizadas(window);
    if lista.iter().any(|p| p == &canon) {
        return;
    }
    lista.push(canon);
    let como_texto: Vec<String> = lista.iter().map(|p| p.to_string_lossy().to_string()).collect();
    if let (Some(p), Ok(s)) = (pastas_autorizadas_path(window), serde_json::to_string_pretty(&como_texto)) {
        let _ = std::fs::write(p, s);
    }
}

/// A decisão, separada do IO para poder ser provada sem uma janela: o alvo é a própria base
/// ou está **dentro** dela.
///
/// `starts_with` de `Path` compara **componentes**, não texto — então `/x/projeto2` não cai
/// dentro de `/x/projeto`, que seria o furo de uma comparação por prefixo de string.
pub(super) fn dentro_de_alguma(alvo: &std::path::Path, bases: &[PathBuf]) -> bool {
    bases.iter().any(|base| alvo == base || alvo.starts_with(base))
}

/// A pasta pedida está dentro de alguma autorizada? Compara caminhos **canônicos**, então
/// symlink e `..` não contornam.
pub(super) fn pasta_autorizada(window: &WebviewWindow, path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    let Ok(alvo) = std::fs::canonicalize(path) else {
        return false;
    };
    let bases: Vec<PathBuf> = pastas_autorizadas(window)
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(p).ok())
        .collect();
    dentro_de_alguma(&alvo, &bases)
}

/// Resposta única para pedido fora da cerca. Não diz se o caminho existe — a página não
/// precisa saber, e responder diferente para "não existe" e "não autorizado" seria um
/// oráculo de sistema de arquivos para quem já está pedindo o que não devia.
pub(super) fn fora_da_cerca() -> serde_json::Value {
    serde_json::json!({
        "ok": false,
        "erro": "pasta não autorizada — escolha a pasta do projeto pelo botão do app",
        "codigo": "pasta_nao_autorizada",
    })
}

// ── vínculo projeto→pasta (config local do dispositivo) ─────────────────────

pub(super) fn bindings_path(window: &WebviewWindow) -> Option<PathBuf> {
    let dir = window.app_handle().path().app_config_dir().ok()?;
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("modo-code-bindings.json"))
}

pub(super) fn load_bindings(window: &WebviewWindow) -> serde_json::Map<String, serde_json::Value> {
    bindings_path(window)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub(super) fn set_binding(window: &WebviewWindow, v: &serde_json::Value) {
    let pid = v.get("projectId").and_then(|x| x.as_str()).unwrap_or_default();
    let path = v.get("path").and_then(|x| x.as_str()).unwrap_or_default();
    if pid.is_empty() {
        return;
    }
    let mut map = load_bindings(window);
    map.insert(pid.to_string(), serde_json::json!(path));
    if let (Some(p), Ok(s)) = (bindings_path(window), serde_json::to_string_pretty(&map)) {
        let _ = std::fs::write(p, s);
    }
}


/// A cerca do Modo Code (F-12): o que decide é a lista de pastas que o usuário escolheu no
/// diálogo nativo, e a comparação é por componente de caminho CANÔNICO.
#[cfg(test)]
mod tests_cerca {
    use super::dentro_de_alguma;
    use std::path::PathBuf;

    fn sandbox() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let id = N.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("shvia-cerca-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("projeto/sub")).unwrap();
        std::fs::create_dir_all(dir.join("projeto2")).unwrap();
        std::fs::create_dir_all(dir.join("outro")).unwrap();
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn autoriza_a_pasta_escolhida_e_o_que_esta_dentro() {
        let raiz = sandbox();
        let bases = vec![raiz.join("projeto")];
        assert!(dentro_de_alguma(&raiz.join("projeto"), &bases));
        assert!(dentro_de_alguma(&raiz.join("projeto/sub"), &bases));
    }

    #[test]
    fn recusa_o_que_esta_fora() {
        let raiz = sandbox();
        let bases = vec![raiz.join("projeto")];
        for fora in [raiz.join("outro"), raiz.clone(), PathBuf::from("/"), PathBuf::from("/etc")] {
            assert!(!dentro_de_alguma(&fora, &bases), "deveria recusar: {}", fora.display());
        }
    }

    /// O furo que uma comparação por prefixo de STRING teria: `/x/projeto2` começa com
    /// `/x/projeto`. `Path::starts_with` compara componentes e não cai nessa.
    #[test]
    fn vizinho_com_nome_parecido_nao_entra() {
        let raiz = sandbox();
        let bases = vec![raiz.join("projeto")];
        assert!(!dentro_de_alguma(&raiz.join("projeto2"), &bases));
    }

    #[test]
    fn sem_nenhuma_pasta_autorizada_nada_passa() {
        let raiz = sandbox();
        assert!(!dentro_de_alguma(&raiz.join("projeto"), &[]));
    }

    /// `..` e symlink são resolvidos ANTES de comparar (o `pasta_autorizada` canonicaliza);
    /// aqui se prova que a decisão, recebendo o caminho já resolvido, não se deixa enganar.
    #[test]
    fn travessia_resolvida_nao_entra() {
        let raiz = sandbox();
        let bases = vec![raiz.join("projeto")];
        let escapou = std::fs::canonicalize(raiz.join("projeto/sub/../../outro")).unwrap();
        assert!(!dentro_de_alguma(&escapou, &bases));
    }
}
