#!/usr/bin/env node
/**
 * An unprivileged Debian build must describe root-owned files in BOTH the tar and
 * .MTREE. fpm normalizes the tar's owners but otherwise records the builder in
 * .MTREE, making pacman -Qkk report every installed file as altered (1.38.6).
 *
 * Run the real Arch block from build-local.sh against a tiny, real DEB. Nothing is
 * built, installed or published outside the temporary tree. Requires Linux,
 * Node, fpm, fakeroot, dpkg-deb, bsdtar and zstd; running as root would hide the bug.
 */
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gunzipSync } from "node:zlib";

assert.equal(process.platform, "linux", "This packaging proof requires Linux; it must not silently skip.");
assert.notEqual(process.getuid(), 0, "Run as a normal user: root would hide the .MTREE ownership regression.");
const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const source = readFileSync(join(root, "build-local.sh"), "utf8");
const start = source.indexOf("# ── Pacote Arch Linux (.pkg.tar.zst)");
const end = source.indexOf("# Com reuso, a listagem abaixo", start);
assert.ok(start >= 0 && end > start, "The Arch packaging block moved out of build-local.sh.");
const block = source.slice(start, end);
const run = (program, args, options = {}) => execFileSync(program, args, {
  encoding: "utf8", maxBuffer: 8 * 1024 * 1024, timeout: 120_000, ...options,
});
const options = "!all,use-set,type,uid,gid,mode,time,size,md5,sha256,link";

// libarchive emits ./ paths, octal escapes and /set defaults. Decode
// both descriptions independently, so /set uid=1000 is checked for every entry.
function mtree(text) {
  const entries = new Map();
  let defaults = {};
  const decode = (s) => s.replace(/\\([0-7]{3})/g, (_, octal) => String.fromCharCode(parseInt(octal, 8)));
  for (const line of text.split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const [path, ...tokens] = line.trim().split(/\s+/);
    if (path === "/unset") {
      for (const key of tokens) { if (key === "all") defaults = {}; else delete defaults[key]; }
      continue;
    }
    const attrs = Object.fromEntries(tokens.map((token) => {
      const i = token.indexOf("=");
      assert.ok(i > 0, `Unexpected mtree token: ${token}`);
      return [token.slice(0, i), decode(token.slice(i + 1))];
    }));
    if (path === "/set") { Object.assign(defaults, attrs); continue; }
    assert.ok(path.startsWith("./"), `Unexpected mtree path: ${path}`);
    assert.ok(!entries.has(decode(path)), `Duplicate mtree path: ${path}`);
    entries.set(decode(path), { ...defaults, ...attrs });
  }
  assert.ok(entries.size > 0, "The mtree must not be empty.");
  return entries;
}

const temp = mkdtempSync(join(tmpdir(), "shvia pacman mtree "));
try {
  for (const hidden of ["repo-add", "repo-add dpkg-deb", "repo-add fakeroot"]) {
    const dir = join(temp, hidden.replaceAll(" ", "-"));
    const payload = join(dir, "payload");
    const bundles = join(dir, "src-tauri/target/release/bundle");
    for (const path of ["DEBIAN", "usr/bin", "usr/share/shvia-desktop"]) mkdirSync(join(payload, path), { recursive: true });
    for (const path of ["deb", "pacman"]) mkdirSync(join(bundles, path), { recursive: true });
    writeFileSync(join(dir, "version.md"), "0.0.1\n");
    writeFileSync(join(payload, "DEBIAN/control"), "Package: shv-ia\nVersion: 0.0.1\nArchitecture: amd64\nMaintainer: Test <test@example.invalid>\nDescription: Isolated packaging fixture\n");
    writeFileSync(join(payload, "usr/bin/shvia-desktop"), "#!/bin/sh\nprintf 'fixture\\n'\n");
    chmodSync(join(payload, "usr/bin/shvia-desktop"), 0o755);
    writeFileSync(join(payload, "usr/share/shvia-desktop/data with spaces"), "fixture content\n");
    chmodSync(join(payload, "usr/share/shvia-desktop/data with spaces"), 0o644);
    symlinkSync("data with spaces", join(payload, "usr/share/shvia-desktop/link"));
    run("dpkg-deb", ["--build", "--root-owner-group", payload, join(bundles, "deb/shv-ia_0.0.1_amd64.deb")]);

    // Only command discovery is controlled. The extraction, fpm, fakeroot, tar
    // and compressors are real, including the bsdtar extraction fallback.
    const output = run("bash", ["-c", `set -euo pipefail
exec 2>&1
_BUILD_OS=Linux
_LINUX_FAMILIA=debian
command() {
  if [[ "\${1-}" == -v ]]; then
    case " $SHVIA_TEST_HIDDEN_COMMANDS " in *" $2 "*) return 1;; esac
  fi
  builtin command "$@"
}
${block}`], { cwd: dir, env: { ...process.env, SHVIA_TEST_HIDDEN_COMMANDS: hidden } });
    const packages = readdirSync(join(bundles, "pacman")).filter((name) => name.endsWith(".pkg.tar.zst"));
    if (hidden.includes("fakeroot")) {
      assert.deepEqual(packages, [], "Missing fakeroot must not produce a broken package.");
      assert.match(output, /apt install[^\n]*fakeroot/, "Explain how to install the missing build prerequisite.");
      console.log("[pacman-mtree] missing fakeroot: no package generated");
      continue;
    }
    assert.equal(packages.length, 1, output);
    const archive = join(bundles, "pacman", packages[0]);
    const recorded = mtree(gunzipSync(run("bsdtar", ["-xOf", archive, ".MTREE"], { encoding: null })).toString());
    const actual = mtree(run("bsdtar", ["--format=mtree", `--options=${options}`, "-cf", "-", `@${archive}`]));
    actual.delete("./.MTREE"); // A manifest cannot contain its own digest.
    assert.deepEqual([...recorded.keys()].sort(), [...actual.keys()].sort());
    for (const [path, attrs] of recorded) {
      assert.equal(attrs.uid, "0", `${path}: .MTREE UID must be root, not the builder`);
      assert.equal(attrs.gid, "0", `${path}: .MTREE GID must be root, not the builder`);
      const stored = actual.get(path);
      // GNU tar rounds timestamps down to seconds; pacman compares seconds too.
      assert.equal(Math.floor(Number(attrs.time)), Math.floor(Number(stored.time)), `${path}: time`);
      const { time: _recordedTime, ...metadata } = attrs;
      const { time: _storedTime, ...archiveMetadata } = stored;
      assert.deepEqual(metadata, archiveMetadata, `${path}: .MTREE must describe the archived payload`);
    }
    assert.equal(recorded.get("./usr/bin/shvia-desktop").mode, "755");
    assert.equal(recorded.get("./usr/share/shvia-desktop/data with spaces").mode, "644");
    assert.equal(recorded.get("./usr/share/shvia-desktop/link").link, "data with spaces");
    assert.equal(recorded.get("./usr/share/shvia-desktop/instalado-por").mode, "644");
    assert.ok(recorded.has("./.PKGINFO"), "Control metadata must also be root-owned.");
    assert.equal(run("bsdtar", ["-xOf", archive, "usr/share/shvia-desktop/instalado-por"]), "pacman\n");
    const info = run("bsdtar", ["-xOf", archive, ".PKGINFO"]);
    assert.match(info, /^pkgname = shvia-desktop$/m);
    assert.match(info, /^pkgver = 0.0.1-1$/m);
    console.log(`[pacman-mtree] ${hidden.includes("dpkg-deb") ? "bsdtar" : "dpkg-deb"}: ${recorded.size} entries match tar ownership, modes, content and links`);
  }
} finally {
  rmSync(temp, { recursive: true, force: true });
}
