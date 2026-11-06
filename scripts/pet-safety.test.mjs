import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import crypto from 'node:crypto';

test('pet capability has events only and cannot invoke credential commands', () => {
  const cap=JSON.parse(fs.readFileSync('src-tauri/capabilities/pet.json','utf8'));
  assert.deepEqual(cap.permissions,['core:event:allow-listen','core:event:allow-unlisten']);
  const source=fs.readFileSync('src-tauri/src/lib.rs','utf8');
  assert.match(source,/label\(\)=="pet"/);
  assert.match(source,/The pet surface cannot access account secrets/);
});
test('pet creates Windows WebView asynchronously and hiding does not stop services', () => {
  const source=fs.readFileSync('src-tauri/src/pet.rs','utf8');
  assert.match(source,/pub async fn open_pet/);
  assert.doesNotMatch(source,/process::|\.exit\(|Stop-Process|proxy_enabled\s*=/);
});
test('all eight bundled skins match the attribution manifest, with no plugin code', () => {
  const manifest=JSON.parse(fs.readFileSync('public/pets/manifest.json','utf8').replace(/^\uFEFF/,''));
  assert.equal(manifest.assets.length,8);assert.equal(manifest.license,'CC BY-NC 4.0');assert.equal(manifest.author,'fywdy');
  for(const asset of manifest.assets){
    assert.match(asset.file,/^[a-z]+\.webp$/);
    assert.equal(crypto.createHash('sha256').update(fs.readFileSync('public/pets/'+asset.file)).digest('hex').toUpperCase(),asset.sha256);
  }
  assert.ok(fs.existsSync('public/pets/LICENSE-CC-BY-NC-4.0.txt'));
  assert.ok(fs.existsSync('public/pets/NOTICE.md'));
  assert.ok(!fs.readdirSync('public/pets').some(name=>/\.(js|ps1|exe|bat|cmd)$/i.test(name)));
});

test('active chat observer is read-only, ignores subagents and keeps no conversation text',()=>{
  const s=fs.readFileSync('src-tauri/src/activity_scan.rs','utf8');
  assert.match(s,/readonly_codex_sessions_root/);
  assert.match(s,/source\.is_object\(\)/);
  assert.doesNotMatch(s,/fs::write|File::create|OpenOptions|payload.*message/);
  assert.match(s,/turn_context/);
});
test('silent startup is configured before the window is created; local sounds are not synthesized',()=>{
  const config=JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json','utf8'));
  assert.equal(config.app.windows[0].visible,false);
  assert.match(config.app.security.csp,/media-src 'self' data:/);
  const ui=fs.readFileSync('src/components/DesktopPet.tsx','utf8');
  assert.doesNotMatch(ui,/AudioContext|createOscillator/);
  assert.match(ui,/onPointerCancel/);assert.match(ui,/setPointerCapture/);
  assert.ok(!fs.readdirSync('public/pets').some(f=>/\.(mp3|wav)$/i.test(f)));
});

test('disabled OAuth background work cannot bypass controls through Google discovery or priming',()=>{
 const s=fs.readFileSync('src-tauri/src/lib.rs','utf8');
 const google=s.slice(s.indexOf('fn start_antigravity_catalog_refresh('),s.indexOf('pub fn start_quota_refresh('));
 assert.ok(google.indexOf('settings.background_refresh')<google.indexOf('refresh_antigravity_quota_local'));
 const quota=s.slice(s.indexOf('pub fn start_quota_refresh('));
 assert.ok(quota.indexOf('if paused')<quota.indexOf('apply_automatic_window_priming_defaults'));
 assert.match(quota,/!s.settings.quota_refresh_enabled/);
});
