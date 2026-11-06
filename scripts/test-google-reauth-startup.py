"""Exercise the real desktop startup migration with synthetic credentials only."""
import copy
import json
import subprocess
import sys
import time
from pathlib import Path

exe = Path(sys.argv[1]).resolve(strict=True)
assert exe.name == "switcher-pet.exe"
store = Path.home() / ".switcher-pet/validation/profile/.codex-switcher/accounts.json"
previous = store.read_bytes() if store.exists() else None
if previous:
    prior = json.loads(previous)
    assert not prior["accounts"] and not prior["settings"]["proxy_enabled"]
store.parent.mkdir(parents=True, exist_ok=True)
old = {
    "id": "synthetic-old", "name": "Synthetic expired import", "kind": "legacy",
    "created_at": "2020-01-01T00:00:00Z",
    "auth_json": {"provider": "antigravity", "email": "synthetic@example.invalid",
                  "project_id": "synthetic-project", "tokens": {
                      "access_token": "synthetic-expired", "refresh_token": "synthetic-old-refresh",
                      "expires_at": "2000-01-01T00:00:00Z"}},
}
new = copy.deepcopy(old)
new.update(id="synthetic-new", name="Synthetic new login", kind="antigravity_oauth",
           created_at="2026-01-01T00:00:00Z")
new["auth_json"]["tokens"].update(access_token="synthetic-fresh",
                               refresh_token="synthetic-new-refresh",
                               expires_at="2099-01-01T00:00:00Z")
data = {
    "version": 1, "current": None,
    "accounts": {old["id"]: old, new["id"]: new},
    "settings": {
        "proxy_enabled": False, "background_refresh": False, "remote_mode": "off",
        "quota_refresh_enabled": False,
        "current_antigravity_account_id": old["id"],
        "current_google_accounts": {"gemini-synthetic": old["id"]},
        "current_model_sources": {"gemini-synthetic": old["id"], "other-model": "unrelated"},
        "model_route_policies": {"gemini-synthetic": {
            "auto_switch": False, "allowed_sources": ["unrelated", old["id"], new["id"]]}},
    },
}
process = None
try:
    store.write_text(json.dumps(data), encoding="utf8")
    process = subprocess.Popen([str(exe), "--validation"], cwd=exe.parent,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                               creationflags=subprocess.CREATE_NO_WINDOW)
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        assert process.poll() is None, "Desktop exited during migration"
        try:
            actual = json.loads(store.read_text(encoding="utf-8-sig"))
            if actual["settings"]["current_model_sources"]["gemini-synthetic"] == new["id"]:
                break
        except (OSError, ValueError):
            pass
        time.sleep(.2)
    else:
        raise AssertionError("Startup did not repair expired identity binding")
    settings = actual["settings"]
    assert settings["current_google_accounts"]["gemini-synthetic"] == new["id"]
    assert settings["current_antigravity_account_id"] == new["id"]
    assert settings["current_model_sources"]["other-model"] == "unrelated"
    policy = settings["model_route_policies"]["gemini-synthetic"]
    assert policy["allowed_sources"] == ["unrelated", new["id"]]
    assert not policy["auto_switch"]
    for account in (old, new):
        assert actual["accounts"][account["id"]]["auth_json"] == account["auth_json"]
    print("PASS: real startup repairs same-identity routes, preserves source order, manual mode and credentials")
finally:
    if process is not None and process.poll() is None:
        process.terminate()
        process.wait(timeout=15)
    if previous is None:
        store.unlink(missing_ok=True)
    else:
        store.write_bytes(previous)
