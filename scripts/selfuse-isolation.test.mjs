// Source/config regression checks. These do not substitute for native Rust tests.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8');
const config = () => JSON.parse(read('src-tauri/tauri.conf.json'));

test('fork has separate application, executable and deep-link identities', () => {
  assert.equal(config().productName, 'Switcher Pet');
  assert.equal(config().identifier, 'local.switcherpet.desktop');
  assert.deepEqual(config().plugins['deep-link'].desktop.schemes, ['switcherpet']);
  const cargo = read('src-tauri/Cargo.toml');
  assert.match(cargo, /^name = "switcher-pet"$/m);
  assert.match(cargo, /^default-run = "switcher-pet"$/m);
  assert.equal(JSON.parse(read('package.json')).name, 'switcher-pet');
  assert.equal(JSON.parse(read('package-lock.json')).name, 'switcher-pet');
});

test('standard proxy port is opt-in; development and management stay isolated', () => {
  assert.match(read('src-tauri/src/account.rs'), /proxy_enabled: false/);
  assert.equal(config().build.devUrl, 'http://localhost:2420');
  assert.match(read('vite.config.ts'), /port: 2420/);
  assert.match(read('vite.config.ts'), /port: 2421/);
  assert.match(read('src-tauri/src/account.rs'), /fn default_proxy_port\(\) -> u16 \{\s*18080/);
  assert.match(read('src-tauri/src/account.rs'), /fn default_remote_server_port\(\) -> u16 \{\s*28181/);
  for (const file of ['src/components/Settings.tsx', 'src/components/Proxy.tsx']) {
    assert.match(read(file), /\b18080\b/);
    assert.doesNotMatch(read(file), /\b18081\b/);
  }
});

test('only app_paths resolves the real home/config directories', () => {
  const root = new URL('../src-tauri/src/', import.meta.url);
  const paths = [];
  function walk(dir) {
    for (const item of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, item.name);
      if (item.isDirectory()) walk(path);
      else if (item.name.endsWith('.rs')) paths.push(path);
    }
  }
  // URL-safe path conversion is important on Windows.
  const rootPath = fileURLToPath(root);
  walk(rootPath);
  for (const path of paths) {
    if (relative(rootPath, path) === 'app_paths.rs') continue;
    assert.doesNotMatch(readFileSync(path, 'utf8'), /dirs::(?:home_dir|config_dir)\(/, path);
  }
  const account = read('src-tauri/src/account.rs');
  assert.doesNotMatch(account, /var_os\("CODEX_SWITCHER_TEST_HOME"\)/);
  assert.match(account, /crate::app_paths::profile_home\(\)/);
  assert.doesNotMatch(read('src-tauri/src/codex_sessions.rs'), /PathBuf::from\("\.codex\/sessions"\)/);
});

test('startup prepares and locks the isolated profile before loading application state', () => {
  const source = read('src-tauri/src/lib.rs');
  const run = source.slice(source.indexOf('pub fn run() {'));
  assert.ok(run.indexOf('app_paths::prepare_runtime()') >= 0);
  assert.ok(run.indexOf('app_paths::prepare_runtime()') < run.indexOf('AppState::new()'));
  const paths = read('src-tauri/src/app_paths.rs');
  assert.match(paths, /\.switcher-pet/);
  assert.match(paths, /share_mode\(0\)/);
  assert.match(paths, /symlink_metadata/);
  assert.match(paths, /FILE_ATTRIBUTE_REPARSE_POINT/);
  assert.match(paths, /pub struct RuntimeGuard/);
});

test('live-process termination, quarantine mutation and global environment takeover are blocked', () => {
  for (const [file, signature] of [
    ['src-tauri/src/lib.rs', 'fn kill_codex_processes()'],
    ['src-tauri/src/lib.rs', 'fn set_proxy_env('],
    ['src-tauri/src/ide_control.rs', 'pub fn reload_ide('],
    ['src-tauri/src/ide_control.rs', 'pub fn remove_quarantine()'],
  ]) {
    const source = read(file);
    const start = source.indexOf(signature);
    assert.ok(start >= 0, signature);
    const body = source.slice(source.indexOf('{', start) + 1);
    assert.match(body, /^\s*crate::app_paths::deny_live_integration\(/, signature);
  }
});

test('the fork accepts its own deep link without registering original schemes', () => {
  assert.match(read('src-tauri/src/deep_link.rs'), /"switcherpet" => "switcherpet"\.to_string\(\)/);
});
