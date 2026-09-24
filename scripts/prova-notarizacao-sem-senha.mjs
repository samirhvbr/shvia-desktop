#!/usr/bin/env node
/**
 * Notarization picks the App Store Connect API key, and the password leaves argv (1.6.39, E20).
 *
 * The app-specific password went to `notarytool` as `--password`, on the command line of a
 * process that waits minutes for Apple: readable in `ps` by anything on the Mac for that long.
 * Tauri's bundler does the same for the .app with APPLE_ID/APPLE_PASSWORD, so only a different
 * credential closes it. This runs the real `escolhe_credencial_de_notarizacao` and
 * `notariza_arquivo`, cut out of build-local.sh, with a temp HOME and fakes on PATH: `security`
 * answers from the case, and `xcrun` writes its argv to a file so the proof can read exactly
 * what `notarytool` would have been given.
 */
import { execFileSync } from "node:child_process";
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RAIZ = join(dirname(fileURLToPath(import.meta.url)), "..");
const FONTE = readFileSync(join(RAIZ, "build-local.sh"), "utf8");
const ini = FONTE.indexOf("escolhe_credencial_de_notarizacao() {");
const n = FONTE.indexOf("notariza_arquivo() {");
if (ini < 0 || n < 0) {
  console.error("🔴 escolhe_credencial_de_notarizacao or notariza_arquivo moved out of build-local.sh.");
  process.exit(1);
}
const BLOCO = FONTE.slice(ini, FONTE.indexOf("\n}\n", n) + 3);

const SENHA = "fake-app-password-9f3k";
const ISSUER = "69a6de7e-0000-47e3-e053-5b8c7c11a4d1";

function roda({ chaves = [], issuerNoArquivo = false, senhaNoKeychain = false, env = {}, modo = 0o600 }) {
  const dir = mkdtempSync(join(tmpdir(), "shvia-notar-"));
  try {
    const home = join(dir, "home");
    const bin = join(dir, "bin");
    mkdirSync(join(home, ".shvia"), { recursive: true });
    mkdirSync(bin);
    for (const k of chaves) {
      const f = join(home, ".shvia", `AuthKey_${k}.p8`);
      writeFileSync(f, "-----BEGIN PRIVATE KEY-----\nfake\n-----END PRIVATE KEY-----\n");
      chmodSync(f, modo);
    }
    if (issuerNoArquivo) writeFileSync(join(home, ".shvia", "apple-api-issuer"), `${ISSUER}\n`);
    const argv = join(dir, "xcrun-argv");
    writeFileSync(join(bin, "security"), senhaNoKeychain
      ? `#!/bin/sh\ncase "$*" in *-w*) echo '${SENHA}';; *) echo '    "acct"<blob>="dono@example.com"';; esac\n`
      : "#!/bin/sh\nexit 44\n", { mode: 0o755 });
    writeFileSync(join(bin, "xcrun"), `#!/bin/sh\nprintf '%s\\n' "$@" > '${argv}'\n`, { mode: 0o755 });
    const script = [
      "set -euo pipefail",
      'NOTARY_SERVICE="shvia-notarize"; NOTARIZE_ENABLED=0; NOTARIZE_MODE=""; APPLE_TEAM_ID=S65UBCTPN5',
      BLOCO,
      "escolhe_credencial_de_notarizacao",
      'echo "MODE=$NOTARIZE_MODE"',
      "{ env | grep '^APPLE_' || true; } | cut -d= -f1 | sort | sed 's/^/EXPORTED=/'",
      '[ -z "$NOTARIZE_MODE" ] || notariza_arquivo /tmp/ShvIA.dmg',
    ].join("\n");
    const saida = execFileSync("bash", ["-c", script], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
      env: { PATH: `${bin}:/usr/bin:/bin`, HOME: home, ...env },
    });
    const exportados = new Set([...saida.matchAll(/^EXPORTED=(\w+)$/gm)].map((m) => m[1]));
    return {
      status: 0,
      saida,
      modo: saida.match(/^MODE=(\w*)$/m)?.[1] ?? "",
      exportados,
      argv: existsSync(argv) ? readFileSync(argv, "utf8").split("\n").filter(Boolean) : [],
    };
  } catch (e) {
    return { status: e.status ?? 1, saida: `${e.stdout ?? ""}${e.stderr ?? ""}`, modo: "", exportados: new Set(), argv: [] };
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const semSenha = (r) => !r.exportados.has("APPLE_PASSWORD") && !r.exportados.has("APPLE_ID")
  && !r.argv.includes("--password") && !r.argv.includes(SENHA);
const pelaChave = (id) => (r) => r.modo === "api" && semSenha(r)
  && r.argv.includes("--key-id") && r.argv[r.argv.indexOf("--key-id") + 1] === id
  && r.argv.includes("--issuer") && r.argv[r.argv.indexOf("--issuer") + 1] === ISSUER
  && /AuthKey_\w+\.p8$/.test(r.argv[r.argv.indexOf("--key") + 1] ?? "");

const CASOS = [
  ["one key in ~/.shvia + issuer file wins over the keychain password",
    () => roda({ chaves: ["ABCDE12345"], issuerNoArquivo: true, senhaNoKeychain: true }), pelaChave("ABCDE12345")],
  ["🔴 APPLE_ID/APPLE_PASSWORD already exported (signing.env) are UNSET in API mode",
    () => roda({ chaves: ["ABCDE12345"], issuerNoArquivo: true, env: { APPLE_ID: "dono@example.com", APPLE_PASSWORD: SENHA } }),
    pelaChave("ABCDE12345")],
  ["no key: the keychain password still notarizes, with the ps warning",
    () => roda({ senhaNoKeychain: true }),
    (r) => r.modo === "senha" && r.argv.includes("--password") && /ps/.test(r.saida)],
  ["key without issuer: said out loud, then the password",
    () => roda({ chaves: ["ABCDE12345"], senhaNoKeychain: true }),
    (r) => r.modo === "senha" && /incompleta/.test(r.saida) && /Issuer/.test(r.saida)],
  ["🔴 two keys and no APPLE_API_KEY: refuses to guess",
    () => roda({ chaves: ["ABCDE12345", "ZYXWV98765"], issuerNoArquivo: true }), (r) => r.status === 1],
  ["two keys, APPLE_API_KEY picks one",
    () => roda({ chaves: ["ABCDE12345", "ZYXWV98765"], issuerNoArquivo: true, env: { APPLE_API_KEY: "ZYXWV98765" } }),
    pelaChave("ZYXWV98765")],
  ["🔴 APPLE_API_KEY names a missing file: another key is never its stand-in",
    () => roda({ chaves: ["ABCDE12345"], issuerNoArquivo: true, senhaNoKeychain: true, env: { APPLE_API_KEY: "QQQQQ11111" } }),
    (r) => r.modo === "senha" && /incompleta/.test(r.saida) && !r.argv.includes("ABCDE12345")],
  ["everything from the environment",
    () => roda({ chaves: ["ABCDE12345"], env: { APPLE_API_KEY: "ABCDE12345", APPLE_API_ISSUER: ISSUER } }),
    pelaChave("ABCDE12345")],
  ["a key readable by others is flagged, and still used",
    () => roda({ chaves: ["ABCDE12345"], issuerNoArquivo: true, modo: 0o644 }),
    (r) => pelaChave("ABCDE12345")(r) && /chmod 600/.test(r.saida)],
  ["no credential at all: signed, not notarized, no failure",
    () => roda({}), (r) => r.status === 0 && r.modo === "" && r.argv.length === 0],
];

let falhas = 0;
for (const [nome, rodar, ok] of CASOS) {
  const r = rodar();
  if (!ok(r)) {
    falhas++;
    console.error(`🔴 ${nome}\n   status ${r.status}, mode "${r.modo}", exported [${[...r.exportados].join(", ")}], argv [${r.argv.join(" ")}]`);
  }
}
if (falhas) {
  console.error(`\n${falhas} of ${CASOS.length} cases failed: the password could be back on notarytool's command line.`);
  process.exit(1);
}
console.log(`[notarizacao] ${CASOS.length} cases · the API key wins, and in API mode no password is exported or passed`);
