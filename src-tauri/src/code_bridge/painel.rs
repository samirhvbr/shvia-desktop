//! The folder panel (F4): `git status`, the file tree, a file's diff, and a file read.
//! Read-only, and only for a folder inside the fence: `handle_message` checks
//! `pasta_autorizada` (cerca.rs) before calling anything here, and every call goes through
//! `fora_da_ui` with a deadline. Split out of `code_bridge.rs` in 1.6.41.

use super::*;

#[cfg(test)]
mod tests_leitura_com_teto {
    use super::{ler_escolhido, ler_no_maximo, read_file, READ_FILE_MAX};

    fn pasta(nome: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("shvia-teto-{nome}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 🔴 1.6.28. The whole file was read and the limit applied after. A sparse 64 MB file uses
    /// no disk; reading it whole would allocate 64 MB — the cap stops at limit + 1.
    #[test]
    fn arquivo_enorme_para_no_limite() {
        let d = pasta("enorme");
        let p = d.join("video.bin");
        std::fs::File::create(&p).unwrap().set_len(64 << 20).unwrap();
        assert_eq!(ler_no_maximo(&p, 1024).unwrap().len(), 1025, "the read did not stop at the cap");
        assert_eq!(ler_escolhido(&p, 10 << 20).unwrap_err(), "acima de 10 MB");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn arquivo_pequeno_passa_inteiro() {
        let d = pasta("pequeno");
        let p = d.join("nota.txt");
        std::fs::write(&p, b"12345").unwrap();
        assert_eq!(ler_escolhido(&p, 10 << 20).unwrap(), b"12345");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn previa_corta_no_limite_e_diz_que_cortou() {
        let d = pasta("previa");
        std::fs::write(d.join("log.txt"), vec![b'a'; READ_FILE_MAX + 1000]).unwrap();
        // `file` is absolute, like every caller passes it (see the readFile tests).
        let r = read_file(&d.to_string_lossy(), &d.join("log.txt").to_string_lossy());
        assert_eq!(r["truncated"], true);
        assert_eq!(r["content"].as_str().unwrap().len(), READ_FILE_MAX);
        assert_eq!(r["bytes"], (READ_FILE_MAX + 1000) as u64);
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod tests_git_diff {
    use super::{git_diff, git_status, parse_git_status, GIT_DIFF_MAX};
    use std::process::Command;

    /// The `-z` format, entry by entry — including the rename, whose second field is the
    /// OLD path and must not become a file of its own.
    #[test]
    fn status_z_traz_nomes_crus_e_renomeacao_vira_uma_entrada() {
        let saida = "## master...origin/master [ahead 1]\0 M a b.txt\0 M a\u{e7}\u{e3}o.txt\0\
                     RM novo nome.txt\0velho.txt\0?? n\u{e3}o rastreado.txt\0";
        let v = parse_git_status(saida);
        assert_eq!(v["branch"], "master");
        let arquivos: Vec<(String, String)> = v["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| (f["status"].as_str().unwrap().to_string(), f["path"].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(
            arquivos,
            vec![
                ("M".to_string(), "a b.txt".to_string()),
                ("M".to_string(), "a\u{e7}\u{e3}o.txt".to_string()),
                ("RM".to_string(), "novo nome.txt".to_string()),
                ("??".to_string(), "n\u{e3}o rastreado.txt".to_string()),
            ]
        );
    }

    /// 🔴 The user's path, end to end, against a real git: the Changes tab lists the file,
    /// then asks for ITS diff by the name it was given. Until 1.6.10 the listed name was
    /// quoted and escaped, and the diff came back empty for every accented or spaced file.
    /// Fails loudly without git on purpose: a skipped test here would read as green.
    #[test]
    fn arquivo_com_acento_ou_espaco_tem_diff_pelo_nome_que_o_status_lista() {
        let dir = repo_temporario("acentos").expect("this test needs git");
        let p = dir.to_string_lossy().into_owned();
        let git = |args: &[&str]| Command::new("git").args(["-C", &p]).args(args).output().unwrap();
        for nome in ["a\u{e7}\u{e3}o.txt", "a b.txt"] {
            std::fs::write(dir.join(nome), "antes\n").unwrap();
        }
        git(&["add", "-A"]);
        git(&["commit", "-qm", "nomes"]);
        for nome in ["a\u{e7}\u{e3}o.txt", "a b.txt"] {
            std::fs::write(dir.join(nome), "depois\n").unwrap();
        }

        let st = git_status(&p);
        let listados: Vec<String> = st["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap().to_string())
            .collect();
        for nome in ["a\u{e7}\u{e3}o.txt", "a b.txt"] {
            assert!(listados.iter().any(|l| l == nome), "status listed {listados:?}, not {nome:?}");
            let d = git_diff(&p, nome);
            let diff = d["diff"].as_str().unwrap_or("");
            assert!(diff.contains("+depois"), "empty diff for {nome:?} listed as changed: {d}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Repo de verdade num diretório temporário — e não um mock do `Command`.
    /// O que esta função faz é FALAR COM O GIT: um mock provaria que sabemos
    /// montar argumentos, não que o git entende os argumentos que montamos.
    fn repo_temporario(nome: &str) -> Option<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("shvia-gitdiff-{}-{}", nome, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok()?;
        let p = dir.to_string_lossy().into_owned();
        let git = |args: &[&str]| Command::new("git").args(["-C", &p]).args(args).output().ok();
        git(&["init", "-q"])?;
        git(&["config", "user.email", "t@t.tld"])?;
        git(&["config", "user.name", "t"])?;
        std::fs::write(dir.join("a.txt"), "linha 1\nlinha 2\n").ok()?;
        git(&["add", "-A"])?;
        git(&["commit", "-qm", "base"])?;

        Some(dir)
    }

    #[test]
    fn diff_de_arquivo_alterado_traz_as_linhas() {
        let Some(dir) = repo_temporario("alterado") else { return };
        std::fs::write(dir.join("a.txt"), "linha 1\nlinha DOIS\n").unwrap();

        let r = git_diff(&dir.to_string_lossy(), "a.txt");

        assert_eq!(r["ok"], true);
        let d = r["diff"].as_str().unwrap();
        assert!(d.contains("-linha 2"), "faltou a linha removida: {d}");
        assert!(d.contains("+linha DOIS"), "faltou a linha adicionada: {d}");
        assert_eq!(r["truncated"], false);
        assert_eq!(r["staged"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 A guarda que separa "vazio" de "sem mudança".
    ///
    /// `git diff` compara a árvore contra o ÍNDICE: um arquivo já preparado
    /// (`git add`) devolve VAZIO. Sem o segundo comando, o painel diria "sem
    /// alterações" para quem acabou de ver o arquivo listado como alterado.
    #[test]
    fn arquivo_ja_preparado_nao_vira_sem_alteracao() {
        let Some(dir) = repo_temporario("staged") else { return };
        std::fs::write(dir.join("a.txt"), "linha 1\nlinha TRES\n").unwrap();
        let p = dir.to_string_lossy().into_owned();
        Command::new("git").args(["-C", &p, "add", "a.txt"]).output().unwrap();

        let r = git_diff(&p, "a.txt");

        assert_eq!(r["ok"], true);
        assert_eq!(r["staged"], true, "não caiu no --staged");
        assert!(r["diff"].as_str().unwrap().contains("+linha TRES"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Truncar em fronteira de CARACTERE. `texto[..N]` em UTF-8 entra em pânico no
    /// meio de um multibyte — e diff de arquivo em português tem acento em toda
    /// linha, então isso não é caso exótico, é o caso comum.
    /// Truncar em fronteira de CARACTERE.
    ///
    /// ⚠️ **Este teste varia o comprimento da linha de propósito, e a primeira
    /// versão dele não variava.** Com um único tamanho, o corte em `GIT_DIFF_MAX`
    /// caía numa fronteira de caractere por acaso — e a reversão (voltar ao
    /// `texto[..N]` cru) **passava**. Um teste que só falha com sorte não prova
    /// guarda nenhuma. Deslocando o conteúdo byte a byte, algum dos casos põe o
    /// corte no meio de um multibyte, e aí o slice cru entra em pânico.
    ///
    /// Não é caso exótico: diff de arquivo em português tem acento em toda linha.
    #[test]
    fn corte_nao_parte_caractere_acentuado() {
        for deslocamento in 0..4usize {
            let Some(dir) = repo_temporario(&format!("acento{deslocamento}")) else { return };
            // "é"/"ç"/"ã" têm 2 bytes cada. O prefixo de N espaços desloca todo o
            // resto, movendo onde o corte cai dentro da linha.
            let linha = format!("{}éçãéçãéçã\n", " ".repeat(deslocamento));
            let gigante: String = linha.repeat(40_000);
            std::fs::write(dir.join("a.txt"), &gigante).unwrap();

            let r = git_diff(&dir.to_string_lossy(), "a.txt");

            assert_eq!(r["ok"], true);
            assert_eq!(r["truncated"], true, "o diff gigante devia ter sido cortado");
            let d = r["diff"].as_str().unwrap();
            assert!(d.len() <= GIT_DIFF_MAX);
            assert!(d.contains("éçã"), "o conteúdo sumiu no corte");
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn fora_de_repo_git_responde_erro_e_nao_panica() {
        let dir = std::env::temp_dir().join(format!("shvia-nao-repo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "x").unwrap();

        let r = git_diff(&dir.to_string_lossy(), "a.txt");

        assert_eq!(r["ok"], false);
        assert!(r["erro"].is_string());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Entrada vazia não vira `git diff -- ""`, que listaria o repo inteiro.
    #[test]
    fn caminho_ou_arquivo_vazio_e_recusado() {
        assert_eq!(git_diff("", "a.txt")["ok"], false);
        assert_eq!(git_diff("/tmp", "")["ok"], false);
    }

    /// O `--` é o que impede um arquivo chamado `-p` (ou homônimo de branch) de
    /// ser lido como opção ou revisão pelo git.
    #[test]
    fn nome_que_parece_opcao_continua_sendo_caminho() {
        let Some(dir) = repo_temporario("dash") else { return };
        let p = dir.to_string_lossy().into_owned();
        std::fs::write(dir.join("-p"), "antes\n").unwrap();
        Command::new("git").args(["-C", &p, "add", "-A"]).output().unwrap();
        Command::new("git").args(["-C", &p, "commit", "-qm", "add -p"]).output().unwrap();
        std::fs::write(dir.join("-p"), "depois\n").unwrap();

        let r = git_diff(&p, "-p");

        assert_eq!(r["ok"], true);
        assert!(r["diff"].as_str().unwrap().contains("+depois"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests_read_file {
    use super::{read_file, READ_FILE_MAX};

    /// Pasta temporária de verdade — como o teste do git_diff, é o FILESYSTEM que
    /// se está exercendo, não um mock.
    fn pasta(nome: &str) -> Option<std::path::PathBuf> {
        let dir = std::env::temp_dir().join(format!("shvia-readfile-{}-{}", nome, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }

    #[test]
    fn arquivo_de_texto_volta_o_conteudo() {
        let Some(dir) = pasta("texto") else { return };
        std::fs::write(dir.join("a.txt"), "linha 1\nlinha 2\n").unwrap();

        let r = read_file(&dir.to_string_lossy(), &dir.join("a.txt").to_string_lossy());

        assert_eq!(r["ok"], true);
        assert_eq!(r["binary"], false);
        assert_eq!(r["truncated"], false);
        assert_eq!(r["content"], "linha 1\nlinha 2\n");
        assert_eq!(r["bytes"], 16);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 NUL nos primeiros bytes = binário: content vazio, mas o TAMANHO real vai
    /// junto. Mostrar bytes de um binário como texto é pior que dizer "é binário".
    #[test]
    fn arquivo_binario_e_dito_e_nao_vira_texto() {
        let Some(dir) = pasta("bin") else { return };
        std::fs::write(dir.join("x.bin"), [0x89u8, 0x50, 0x00, 0x01, 0x02]).unwrap();

        let r = read_file(&dir.to_string_lossy(), &dir.join("x.bin").to_string_lossy());

        assert_eq!(r["ok"], true);
        assert_eq!(r["binary"], true);
        assert_eq!(r["content"], "");
        assert_eq!(r["bytes"], 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 Arquivo maior que o teto é CORTADO e o diz — é prévia, não editor. E o
    /// corte não pode entrar em pânico num multibyte partido (from_utf8_lossy).
    #[test]
    fn arquivo_grande_e_cortado_e_declarado() {
        let Some(dir) = pasta("grande") else { return };
        // Conteúdo acentuado (2 bytes por caractere) para o corte cair no meio de
        // um multibyte — o from_utf8_lossy tem de resolver sem pânico.
        let gigante: String = "áéíóúç".repeat(READ_FILE_MAX);
        std::fs::write(dir.join("g.txt"), &gigante).unwrap();

        let r = read_file(&dir.to_string_lossy(), &dir.join("g.txt").to_string_lossy());

        assert_eq!(r["ok"], true);
        assert_eq!(r["truncated"], true, "o arquivo gigante devia ter sido cortado");
        assert!(r["content"].as_str().unwrap().len() <= READ_FILE_MAX + 3); // +3: 1 U+FFFD possível
        assert!(r["bytes"].as_u64().unwrap() > READ_FILE_MAX as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 🔴 A cerca: um arquivo FORA da pasta do projeto é recusado, mesmo passando o
    /// caminho absoluto dele. É a guarda que separa "ler o projeto" de "ler o disco".
    #[test]
    fn arquivo_fora_da_pasta_e_recusado() {
        let Some(dir) = pasta("cerca") else { return };
        let fora = std::env::temp_dir().join(format!("shvia-fora-{}.txt", std::process::id()));
        std::fs::write(&fora, "segredo").unwrap();

        let r = read_file(&dir.to_string_lossy(), &fora.to_string_lossy());

        assert_eq!(r["ok"], false, "leu um arquivo fora da pasta do projeto");
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(&fora);
    }

    #[test]
    fn pasta_nao_e_arquivo() {
        let Some(dir) = pasta("dir") else { return };
        std::fs::create_dir_all(dir.join("sub")).unwrap();

        let r = read_file(&dir.to_string_lossy(), &dir.join("sub").to_string_lossy());

        assert_eq!(r["ok"], false);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub(super) fn git_status(path: &str) -> serde_json::Value {
    if path.is_empty() {
        return serde_json::json!({ "repo": false });
    }
    // `-z`: without it git QUOTES any path with a space or a non-ASCII byte
    // (` M "a\303\247\303\243o.txt"`), the page passed that quoted string back to
    // `gitDiff`, and git answered 0 bytes — every accented or spaced file showed "no
    // changes" until 1.6.10. With `-z` paths come verbatim, NUL-terminated.
    let mut st = Command::new("git");
    st.args(["-C", path, "status", "--porcelain=v1", "-z", "-b"]);
    match saida_com_prazo(st, PRAZO_GIT) {
        Ok(o) if o.status.success() => parse_git_status(&String::from_utf8_lossy(&o.stdout)),
        _ => serde_json::json!({ "repo": false }), // não é repo git
    }
}

/// Parses `git status --porcelain=v1 -z -b`: NUL-terminated entries, paths verbatim.
///
/// A rename or copy is TWO entries — `RM new\0old\0` — and the second (the old path) is
/// not a file of its own: it is skipped, and the entry keeps the NEW path, which is the
/// one `gitDiff` can open.
pub(super) fn parse_git_status(text: &str) -> serde_json::Value {
    let mut branch = String::new();
    let mut files = Vec::new();
    let mut campos = text.split('\0');
    while let Some(campo) = campos.next() {
        if let Some(rest) = campo.strip_prefix("## ") {
            branch = rest.split("...").next().unwrap_or("").split(' ').next().unwrap_or("").to_string();
            continue;
        }
        let (Some(status), Some(path)) = (campo.get(..2), campo.get(3..)) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        if status.contains('R') || status.contains('C') {
            campos.next();
        }
        files.push(serde_json::json!({ "status": status.trim(), "path": path }));
    }
    serde_json::json!({ "repo": true, "branch": branch, "files": files })
}

/// Teto do diff devolvido à página, em bytes.
///
/// Um diff de arquivo gerado (lockfile, bundle, migration de dados) chega a
/// megabytes, e o painel o pinta LINHA A LINHA no DOM — o custo não é a
/// transferência, é o navegador. 256 KB já são ~4 mil linhas: quem precisa de mais
/// que isso para revisar não está revisando, está procurando.
pub(super) const GIT_DIFF_MAX: usize = 262_144;

/// Teto de leitura de UM arquivo para a prévia da aba "Arquivos". É prévia, não
/// editor: arquivo maior é cortado e a página diz que cortou. Fica na mesma ordem
/// de grandeza do diff (256 KB), um pouco maior porque um arquivo inteiro tem mais
/// contexto que um diff.
pub(super) const READ_FILE_MAX: usize = 512 * 1024;

/// Diff de UM arquivo, para a aba "Alterações" abrir ao clique (item F6.B1).
///
/// ## Por que aqui, e não pedindo ao agente
///
/// O `anna` tem a ferramenta `git_diff`, e usá-la seria "de graça" em linhas de
/// código. Não é: seria uma **inferência paga para preencher um painel** — e o
/// painel se atualiza sozinho ao voltar o foco da janela, então cada alt-tab
/// viraria uma chamada de modelo. Painel é leitura de estado; o precedente certo é
/// o `git_status` logo acima, não o agente.
///
/// ## O `--` não é enfeite
///
/// Sem ele, um arquivo chamado `-p` ou que colida com um nome de branch faria o git
/// interpretá-lo como opção ou revisão. O separador diz "daqui em diante é caminho",
/// e é a única guarda necessária: o nome vem do `git status` do MESMO repositório,
/// não de digitação livre.
///
/// ## Vazio ≠ sem mudança
///
/// `git diff` mostra a árvore de trabalho contra o ÍNDICE. Um arquivo já preparado
/// (`git add`) devolve vazio, e mostrar "sem alterações" ali seria mentir para quem
/// acabou de ver o arquivo listado como alterado. Por isso o segundo comando com
/// `--staged` e o campo `staged` na resposta — a página decide o que dizer, mas
/// recebe a verdade.
pub(super) fn git_diff(path: &str, file: &str) -> serde_json::Value {
    if path.is_empty() || file.is_empty() {
        return serde_json::json!({ "ok": false, "erro": "caminho ou arquivo vazio" });
    }

    let rodar = |staged: bool| -> Option<String> {
        let mut args: Vec<&str> = vec!["-C", path, "diff"];
        if staged {
            args.push("--staged");
        }
        args.extend_from_slice(&["--no-color", "--", file]);
        let mut diff = Command::new("git");
        diff.args(&args);
        match saida_com_prazo(diff, PRAZO_GIT) {
            Ok(o) if o.status.success() => Some(String::from_utf8_lossy(&o.stdout).into_owned()),
            _ => None,
        }
    };

    let bruto = match rodar(false) {
        Some(t) => t,
        None => return serde_json::json!({ "ok": false, "erro": "git diff falhou nesta pasta" }),
    };

    // Árvore de trabalho limpa: tenta o índice antes de concluir "sem mudança".
    let (texto, staged) = if bruto.trim().is_empty() {
        match rodar(true) {
            Some(t) if !t.trim().is_empty() => (t, true),
            _ => (bruto, false),
        }
    } else {
        (bruto, false)
    };

    // Corta em fronteira de CARACTERE: `texto[..N]` em UTF-8 entra em pânico no meio
    // de um multibyte, e diff de arquivo em português tem acento em toda linha.
    let truncated = texto.len() > GIT_DIFF_MAX;
    let texto = if truncated {
        let mut fim = GIT_DIFF_MAX;
        while fim > 0 && !texto.is_char_boundary(fim) {
            fim -= 1;
        }
        texto[..fim].to_string()
    } else {
        texto
    };

    serde_json::json!({ "ok": true, "diff": texto, "truncated": truncated, "staged": staged })
}

/// Um nível da árvore (lazy-load ao expandir). Ignora pastas de build/deps.
pub(super) fn list_tree(path: &str) -> serde_json::Value {
    const IGNORE: &[&str] = &[".git", "node_modules", "vendor", "target", "dist", "build", ".svn"];
    let mut entries: Vec<(bool, String, String)> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(path) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if IGNORE.contains(&name.as_str()) {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            entries.push((is_dir, name, e.path().to_string_lossy().into_owned()));
        }
    }
    // pastas primeiro, depois alfabético.
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase())));
    let arr: Vec<serde_json::Value> = entries
        .into_iter()
        .map(|(d, name, p)| serde_json::json!({ "name": name, "path": p, "isDir": d }))
        .collect();
    serde_json::json!({ "entries": arr })
}

/// Lê UM arquivo para a prévia da aba "Arquivos" abrir ao clique.
///
/// ## Por que aqui, e não pedindo ao agente
///
/// Mesma razão do `git_diff`: o `anna` sabe ler arquivo, mas isso seria uma
/// **inferência paga para preencher um painel**. Prévia é leitura de estado — o
/// caminho certo é este, ao lado do `list_tree` que já listou o arquivo.
///
/// ## A cerca é EXPLÍCITA, ao contrário do list_tree
///
/// O nome vem do `list_tree` do mesmo host, então é confiável — mas **ler arquivo
/// é mais perigoso que listar**, então a guarda não fica implícita: canonicaliza a
/// pasta e o alvo e exige que o alvo esteja DENTRO da pasta. Um `..` que escapasse
/// (ou um symlink que apontasse para fora) é recusado antes da leitura. É a mesma
/// postura do `--` do `git_diff`: o dado é confiável, a cerca existe mesmo assim.
///
/// ## Binário não vira texto vazio
///
/// NUL nos primeiros 8 KB é o heurístico clássico do próprio git. Um binário volta
/// `binary:true` com o conteúdo vazio — mostrar bytes de um PNG como texto seria
/// pior que dizer "é binário". `bytes` é o tamanho REAL, para a página dizer o
/// tamanho mesmo quando não mostra o conteúdo.
pub(super) fn read_file(path: &str, file: &str) -> serde_json::Value {
    if path.is_empty() || file.is_empty() {
        return serde_json::json!({ "ok": false, "erro": "caminho ou arquivo vazio" });
    }

    let base = match std::fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return serde_json::json!({ "ok": false, "erro": "pasta não encontrada" }),
    };
    let alvo = match std::fs::canonicalize(file) {
        Ok(p) => p,
        Err(_) => return serde_json::json!({ "ok": false, "erro": "arquivo não encontrado" }),
    };
    if !alvo.starts_with(&base) {
        return serde_json::json!({ "ok": false, "erro": "arquivo fora da pasta do projeto" });
    }

    let meta = match std::fs::metadata(&alvo) {
        Ok(m) => m,
        Err(_) => return serde_json::json!({ "ok": false, "erro": "não consegui ler o arquivo" }),
    };
    if meta.is_dir() {
        return serde_json::json!({ "ok": false, "erro": "isto é uma pasta, não um arquivo" });
    }
    let bytes_total = meta.len() as usize;

    let dados = match ler_no_maximo(&alvo, READ_FILE_MAX) {
        Ok(d) => d,
        Err(_) => return serde_json::json!({ "ok": false, "erro": "não consegui ler o arquivo" }),
    };

    // Binário: NUL nos primeiros 8 KB. Conteúdo vazio, mas o tamanho real vai junto.
    let amostra = &dados[..dados.len().min(8192)];
    if amostra.contains(&0) {
        return serde_json::json!({
            "ok": true, "content": "", "truncated": false, "binary": true, "bytes": bytes_total,
        });
    }

    // Corte por bytes; `from_utf8_lossy` resolve um multibyte partido na fronteira
    // (vira U+FFFD), sem pânico e sem a aritmética de char_boundary do diff.
    let truncated = dados.len() > READ_FILE_MAX;
    let fatia = &dados[..dados.len().min(READ_FILE_MAX)];
    let texto = String::from_utf8_lossy(fatia).into_owned();

    serde_json::json!({
        "ok": true, "content": texto, "truncated": truncated, "binary": false, "bytes": bytes_total,
    })
}

/// Reads at most `max + 1` bytes: enough to know a file is over the limit without holding it
/// (1.6.28). Before, `fs::read` pulled the WHOLE file into memory and the limit was applied
/// after — a multi-GB log in the project, or a video picked by mistake, allocated its full
/// size (an allocation failure aborts the process) just to be cut or skipped.
pub(super) fn ler_no_maximo(path: &std::path::Path, max: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)?.take(max as u64 + 1).read_to_end(&mut buf)?;
    Ok(buf)
}

/// A picked file, or why it was skipped. The size comes from metadata BEFORE any read, and the
/// read is capped too (a file can grow between the two).
pub(super) fn ler_escolhido(path: &std::path::Path, max: usize) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > max as u64 {
        return Err("acima de 10 MB".to_string());
    }
    let bytes = ler_no_maximo(path, max).map_err(|e| e.to_string())?;
    if bytes.len() > max {
        return Err("acima de 10 MB".to_string());
    }
    Ok(bytes)
}
