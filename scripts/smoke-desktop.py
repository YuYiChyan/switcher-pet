"""Attach to the actual preview EXE's WebView2, not a mocked web page."""
import json
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright, expect

endpoint, output = sys.argv[1:3]
out = Path(output)
out.mkdir(parents=True, exist_ok=True)
with sync_playwright() as p:
    browser = p.chromium.connect_over_cdp(endpoint)
    pages = [page for context in browser.contexts for page in context.pages]
    page = next((page for page in pages if "tauri.localhost" in page.url), pages[0] if pages else None)
    if page is None:
        raise AssertionError("No Tauri main page in the application's WebView")
    # CDP attachment can initially report about:blank while frame metadata arrives.
    page.wait_for_url("**tauri.localhost/**", timeout=30000)
    errors = []
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.wait_for_load_state("networkidle")
    expect(page.locator(".isolated-preview-notice")).to_be_visible(timeout=30000)
    assert page.evaluate("window.__TAURI_INTERNALS__.invoke('get_isolated_preview_mode')") is True
    accounts = page.evaluate("window.__TAURI_INTERNALS__.invoke('get_accounts')")
    assert accounts == [], "Preview must not load real or synthetic accounts"
    status = page.evaluate("window.__TAURI_INTERNALS__.invoke('get_proxy_status')")
    assert status["is_running"] is False
    settings = page.evaluate("window.__TAURI_INTERNALS__.invoke('get_settings')")
    assert settings["background_refresh"] is False
    assert settings["quota_refresh_enabled"] is False
    assert settings["remote_mode"] == "off"
    expect(page.locator(".app-header h1")).to_contain_text("Switcher Pet")
    nav = page.locator(".header-nav button")
    labels = nav.all_text_contents()
    assert len(labels) >= 3, "Navigation did not render"
    # Exercise actual React navigation and IPC-backed settings, without saving.
    settings_button = page.get_by_role("button", name="设置", exact=True)
    if settings_button.count() == 0:
        settings_button = page.get_by_role("button", name="Settings", exact=True)
    settings_button.click()
    expect(page.locator(".settings-page")).to_be_visible(timeout=15000)
    page.screenshot(path=str(out / "settings.png"), full_page=True)
    nav.first.click()
    expect(page.locator(".settings-page")).to_have_count(0)
    expect(page.locator(".isolated-preview-notice")).to_be_visible()
    page.screenshot(path=str(out / "preview.png"), full_page=True)
    assert not errors, f"Frontend runtime errors: {errors}"
    report = {"native_webview": True, "preview": True, "account_count": len(accounts),
              "proxy_running": status["is_running"], "settings_navigation": True,
              "frontend_errors": errors, "navigation": [s.strip() for s in labels]}
    (out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(report, ensure_ascii=True))
    browser.close()
