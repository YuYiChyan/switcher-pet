import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Account } from '../hooks/useAccounts';
import { watchModelRoutes } from './modelRoutesRefresh';
import './ModelRoutes.css';

type Policy = { enabled: boolean; visible: boolean; auto_switch: boolean; allowed_sources: string[] };
type Identity = { plan_label: string; scope: string; email: string | null; workspace_name: string | null;
  workspace_ref: string | null; member_ref: string; status: string; subscription_until: string | null;
  checked_at: string | null; stale: boolean };
type Model = { model: string; slug: string; current: string; policy: Policy; sources: {
  id: string; name: string; protocol: string; upstream_model: string; automatic_supported: boolean;
  identity?: Identity; authorization_status?: string; menu_label?: string }[] };
const localTime = (value: string) => {
  const date = new Date(value); return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
};

export function ModelRoutes({ accounts, onAddSource }: { accounts: Account[]; onAddSource: () => void }) {
  const [models, setModels] = useState<Model[]>([]);
  const [selected, setSelected] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [filter, setFilter] = useState('');
  const load = async () => { setModels(await invoke<Model[]>('get_model_routes')); setError(''); };
  useEffect(() => {
    let disposed = false;
    let loading = false;
    const refresh = () => {
      if (disposed || loading) return;
      loading = true;
      void load().catch(e => { if (!disposed) setError(String(e)); }).finally(() => { loading = false; });
    };
    refresh();
    const subscription = listen('accounts-updated', refresh);
    const stopWatching = watchModelRoutes(refresh);
    return () => { disposed = true; stopWatching(); void subscription.then(unlisten => unlisten()); };
  }, []);
  const model = models.find(m => m.model === selected) ?? models[0];
  const run = async (action: () => Promise<unknown>) => {
    setBusy(true); setError('');
    try { await action(); await load(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  const policy = (next: Policy) => model && run(() => invoke('set_model_route_policy', { model: model.model, policy: next }));
  const discover = () => run(async () => {
    const failures: string[] = [];
    for (const source of accounts.filter(a => a.kind === 'relay')) {
      try { await invoke('refresh_relay_models', { id: source.id }); }
      catch (e) { failures.push(`${source.name}: ${String(e)}`); }
    }
    if (failures.length) throw new Error(failures.join('\n'));
  });
  return <section className="model-routes">
    <header className="model-routes-heading">
      <div><h2>模型与来源</h2><p>先选模型，再决定由谁提供。自动切换不改变模型或推理强度。</p></div>
      <div className="model-routes-actions">
        <button disabled={busy || !accounts.some(a => a.kind === 'relay')} onClick={discover}>探测模型</button>
        <button disabled={busy} onClick={onAddSource}>添加来源</button>
      </div>
    </header>
    {error && <p role="alert" className="model-routes-error">{error}</p>}
    {!models.length ? <div className="model-routes-empty">
      <h3>还没有可路由的模型</h3><p>添加 API 来源并填写模型名称，或点击探测模型。不会自动启用已迁移的账号。</p>
      <button onClick={onAddSource}>添加来源</button>
    </div> : <div className="model-routes-workspace">
      <aside>
        <input aria-label="搜索模型" placeholder="搜索模型" value={filter} onChange={e => setFilter(e.target.value)} />
        <div className="model-routes-list" aria-label="模型列表">
          {models.filter(m => m.model.toLowerCase().includes(filter.toLowerCase())).map(m =>
            <button key={m.model} aria-pressed={m.model === model?.model} onClick={() => setSelected(m.model)}>
              <strong>{m.model}</strong><span>{!m.policy.enabled ? '已禁用' : !m.policy.visible ? '目录隐藏' : m.sources.find(s => s.id === m.current)?.menu_label ?? m.sources.find(s => s.id === m.current)?.name ?? '—'}</span>
            </button>)}
        </div>
      </aside>
      {model && <article>
        <h3>{model.model}</h3><code className="model-route-slug">{model.slug}</code>
        <fieldset disabled={busy}>
          <legend>当前来源与自动切换</legend>
          <div className="model-enable-controls">
            <label><input type="checkbox" aria-label="允许新请求" checked={model.policy.enabled} onChange={e => policy({ ...model.policy, enabled:e.target.checked })} />允许新请求</label>
            <label><input type="checkbox" aria-label="在模型目录显示" checked={model.policy.visible} onChange={e => policy({ ...model.policy, visible:e.target.checked })} />在模型目录显示</label>
          </div>
          <p className="model-route-note">隐藏只影响目录；禁止新请求会明确报错，正在输出的回合不中断。探测模型不会重置这两个选择。</p>
          <label className="model-auto-switch"><input type="checkbox" checked={model.policy.auto_switch}
            onChange={e => policy({ ...model.policy, auto_switch: e.target.checked })} />允许同模型自动切换</label>
          <p className="model-route-note">仅在明确限流时，尝试下方勾选的同模型来源。指定账号、会话绑定及来源专属上下文不会自动换源。</p>
          <table><thead><tr><th>当前来源</th><th>协议</th><th>允许自动切换</th></tr></thead><tbody>
            {model.sources.map(source => <tr key={source.id}>
              <td><label><input type="radio" name="model-source" checked={model.current === source.id}
                onChange={() => run(() => invoke('set_model_source', { id: source.id, model: model.model }))} />{source.name}</label>
                {source.identity ? <div className="model-source-identity">
                  <div className="model-source-badges"><strong>{source.identity.plan_label}</strong>
                    <span>{source.identity.scope}</span><span>{source.identity.status}</span></div>
                  <small>登录账号：{source.identity.email ?? '未返回邮箱'} · 席位 {source.identity.member_ref}</small>
                  <small>{source.identity.scope}：{source.identity.workspace_name ? `${source.identity.workspace_name} · ` : ''}{source.identity.workspace_ref ?? '标识未返回'}</small>
                  {source.identity.subscription_until && <small>订阅日期至 {localTime(source.identity.subscription_until)}</small>}
                  <small>{source.authorization_status} · {source.identity.checked_at ? `最近查询 ${localTime(source.identity.checked_at)}` : '尚无查询时间'}{source.identity.stale ? '（缓存待更新）' : ''}</small>
                </div> : source.protocol === 'openai_key' ? <small className="model-source-identity">OpenAI API · 按量计费，不属于 ChatGPT 订阅</small> : null}
              </td>
              <td>{source.protocol}{source.upstream_model !== model.model && <small style={{ display: 'block' }}>→ {source.upstream_model}</small>}</td>
              <td><input type="checkbox" aria-label={`${source.name} auto`} disabled={!source.automatic_supported}
                checked={model.policy.allowed_sources.includes(source.id)} onChange={e => policy({ ...model.policy,
                  allowed_sources: e.target.checked ? [...model.policy.allowed_sources, source.id] : model.policy.allowed_sources.filter(id => id !== source.id) })} /></td>
            </tr>)}
          </tbody></table>
        </fieldset>
        <p className="model-route-note">未勾选的来源不会被自动使用。没有可用来源时保留错误，不降级到其他模型。</p>
        {model.sources.some(s => s.identity) && <p className="model-route-note">同一登录账号可有不同个人空间／工作区；同一工作区也可有多个席位。订阅状态来自缓存记录，授权失效不等于订阅到期。</p>}
      </article>}
    </div>}
  </section>;
}
