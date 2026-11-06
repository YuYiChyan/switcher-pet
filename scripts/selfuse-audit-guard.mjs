// Source-level regression guard only; behavior is tested by test-routing-desktop.py.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const source = readFileSync(new URL('../src-tauri/src/proxy.rs', import.meta.url), 'utf8');
const begin = source.indexOf('async fn handle_named_relay_response(');
const end = source.indexOf('\nfn is_responses_compact_path', begin);
assert.ok(begin >= 0 && end > begin);
const handler = source.slice(begin, end);
assert.ok(handler.includes('(!path_only.ends_with("/responses") && !is_compact)'), 'Initial guard changed; re-audit');
assert.ok(!handler.includes('if method != Method::POST || !path.split(\'?\').next().unwrap_or("").ends_with("/responses")'), 'Do not reintroduce the second compact-rejecting guard');
assert.ok(handler.includes('forward_native_compact'), 'Compaction forwarder missing');

assert.ok(handler.includes('resolve_hard_route'));
assert.ok(handler.includes('native_failover_candidates'));
const translator = readFileSync(new URL('../src-tauri/src/relay_translate.rs', import.meta.url), 'utf8');
assert.ok(!translator.includes('std::fs::write("/tmp/codex-relay-translate-debug.json"'), 'Do not dump conversation bodies');
console.log('Source guards pass; use native compatibility and desktop routing tests for behavior.');
