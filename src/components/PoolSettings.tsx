import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './PoolSettings.css';

type Policy = { included: boolean; short_weight: number | null; weekly_weight: number | null; short_window: string; billing_group: string; probe_models: boolean; balance_adapter: string; balance_reference: number | null; model_minutes: number; balance_minutes: number };
type Window = { remaining: number; capacity: number; known: number; unknown: number };
type Summary = { members: number; subscription_members: number; duplicates: number; show_short: boolean; short_uncertain: boolean; short: Window; weekly: Window; balances: Record<string, number>; balance_unknown: number; api_sources: number; api_models: number; unavailable: number };
type Row = { id: string; name: string; subscription: boolean; plan: string; policy: Policy; default_weight: number; probe_supported: boolean; unavailable: boolean; summary: Summary };
type Probe = { last_at: number; next_at: number; status: string };
type Data = { summary: Summary; accounts: Row[]; probes: Record<string, Probe> };
const number = (v: number) => new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(v);

function QuotaLine({ label, value }: { label: string; value: Window }) {
  return <div className="pool-ledger-line"><span>{label}</span><strong>{value.known ? `≈ ${number(value.remaining)}%` : '未知'}</strong>
    <small>总容量 ≈ {number(value.capacity)}% · 已知 {value.known} · 未知 {value.unknown}</small></div>;
}

export function PoolSettings() {
  const [data, setData] = useState<Data | null>(null);
  const [selected, setSelected] = useState('');
  const [display, setDisplay] = useState('pool');
  const [error, setError] = useState('');
  const load = async () => setData(await invoke<Data>('get_pool_dashboard'));
  useEffect(() => {
    let disposed = false;
    const refresh = () => { if (!disposed) void load().catch(e => setError(String(e))); };
    refresh(); const subscription = listen('accounts-updated', refresh);
    const timer = window.setInterval(refresh, 15000);
    return () => { disposed = true; clearInterval(timer); void subscription.then(fn => fn()); };
  }, []);
  const row = data?.accounts.find(a => a.id === selected) ?? data?.accounts[0];
  const summary = display === 'pool' ? data?.summary : row?.summary;
  return <section className="pool-settings">
    <header><div><h2>额度与探针</h2><p>Plus 等效估算，可修改倍率。订阅额度与 API 余额分开计算。</p></div>
      <div className="pool-display"><button aria-pressed={display === 'pool'} onClick={() => setDisplay('pool')}>总池</button><button aria-pressed={display === 'account'} onClick={() => setDisplay('account')}>独立账号</button></div>
    </header>
    {error && <p role="alert">{error}</p>}
    {summary && <div className="pool-ledger" aria-label="额度概览">
      {summary.members === 0 ? <p>暂无入池账号</p> : <>
        {summary.show_short && <QuotaLine label={summary.short_uncertain ? '五小时 · 限制待确认' : '五小时 · 仅受限账号子池'} value={summary.short} />}
        {summary.subscription_members > 0 && <QuotaLine label="周额度" value={summary.weekly} />}
        <div className="pool-ledger-line"><span>可调用 API</span><strong>{summary.api_sources} <small>来源</small> / {summary.api_models} <small>模型</small></strong></div>
        <div className="pool-balances"><span>余额与预算</span><div>{Object.entries(summary.balances).map(([unit, amount]) => <p key={unit}>{unit.replace('balance / ', '余额 / ').replace('budget / ', '预算 / ').replace('quota / ', '额度 / ')} <strong>{number(amount)}</strong></p>)}
          {!Object.keys(summary.balances).length && <p>暂无已知余额</p>}<small>未知余额来源 {summary.balance_unknown} · 重复订阅 {summary.duplicates} · 暂不可用 {summary.unavailable}</small></div></div>
      </>}
      <p className="pool-note">约等于值不是官方承诺。未知或过期数据不按零计算；周库存不等于此刻可一次调用的量。</p>
    </div>}
    {data?.accounts.length ? <div className="pool-workbench">
      <aside aria-label="额度账号列表">{data.accounts.map(a => <button key={a.id} aria-pressed={a.id === row?.id} onClick={() => setSelected(a.id)}><strong>{a.name}</strong><span>{a.policy.included ? '已入池' : '未入池'} · {a.subscription ? a.plan || '订阅' : 'API / 其他'}</span></button>)}</aside>
      {row && <SourceEditor key={row.id} row={row} probes={data.probes} refresh={load} />}
    </div> : <p>添加来源后可配置入池、倍率和自动探针。已复制的真实账号不会自动启用。</p>}
  </section>;
}

function SourceEditor({ row, probes, refresh }: { row: Row; probes: Record<string, Probe>; refresh: () => Promise<void> }) {
  const [draft, setDraft] = useState(row.policy);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [saved, setSaved] = useState(false);
  const update = (patch: Partial<Policy>) => { setDraft(d => ({ ...d, ...patch })); setDirty(true); setSaved(false); };
  useEffect(() => { if (!dirty) setDraft(row.policy); }, [row.policy, dirty]);
  const run = async (probe: boolean) => {
    setBusy(true); setError(''); setSaved(false);
    try {
      if (!probe) { await invoke('set_source_policy', { id: row.id, policy: draft }); setDirty(false); setSaved(true); }
      else await invoke('probe_source_now', { id: row.id });
      await refresh();
    } catch (e) { setError(String(e)); await refresh(); } finally { setBusy(false); }
  };
  return <form onSubmit={e => { e.preventDefault(); void run(false); }}>
    <h3>{row.name}</h3>
    {error && <p role="alert">{error}</p>}
    <fieldset disabled={busy}><legend>统计规则</legend>
      <label className="pool-check"><input type="checkbox" checked={draft.included} onChange={e => update({ included: e.target.checked })} />纳入总池</label>
      {row.subscription && <>
        <p className="pool-note">默认倍率 ≈ {row.default_weight}x；留空恢复默认，支持 1 / 10 / 20 / 50 / 100x 和自定义。</p>
        <div className="pool-fields"><label>五小时倍率<input aria-label="五小时倍率" type="number" min="0.01" max="10000" step="any" list="pool-weights" placeholder={String(row.default_weight)} value={draft.short_weight ?? ''} onChange={e => update({ short_weight:e.target.value === '' ? null : Number(e.target.value) })} /></label>
          <label>周倍率<input aria-label="周倍率" type="number" min="0.01" max="10000" step="any" list="pool-weights" placeholder={String(row.default_weight)} value={draft.weekly_weight ?? ''} onChange={e => update({ weekly_weight:e.target.value === '' ? null : Number(e.target.value) })} /></label></div>
        <datalist id="pool-weights">{[1,10,20,50,100].map(v => <option key={v} value={v} />)}</datalist>
        <label>五小时限制<select aria-label="五小时限制" value={draft.short_window} onChange={e => update({ short_window:e.target.value })}><option value="auto">按实际窗口检测</option><option value="limited">手动确认有此限制</option><option value="none">手动确认无此限制</option></select></label>
      </>}
      <label>共享计费组<input aria-label="共享计费组" maxLength={128} value={draft.billing_group} onChange={e => update({ billing_group:e.target.value })} /></label>
      {row.probe_supported&&<><label>余额百分比基准<input aria-label="余额百分比基准" type="number" min="0.01" step="any" value={draft.balance_reference??''} onChange={e=>update({balance_reference:e.target.value===''?null:Number(e.target.value)})}/></label><p className="pool-note">留空使用接口总额度；接口未提供时，以首次余额及后续充值峰值估算。单位与余额一致，估算显示约等于号。</p></>}
      <p className="pool-note">同一余额或订阅的多个账号填相同组名，只统计一次；不同币种不换算相加。</p>
    </fieldset>
    {row.probe_supported && <fieldset disabled={busy}><legend>只读自动探针</legend>
      <label className="pool-check"><input aria-label="自动探测模型" type="checkbox" checked={draft.probe_models} onChange={e => update({ probe_models:e.target.checked })} />自动探测模型</label>
      <div className="pool-fields"><label>模型间隔（分钟）<input aria-label="模型间隔（分钟）" type="number" min="1" max="10080" required value={draft.model_minutes} onChange={e => update({ model_minutes:Number(e.target.value) })} /></label>
        <label>余额间隔（分钟）<input aria-label="余额间隔（分钟）" type="number" min="1" max="10080" required value={draft.balance_minutes} onChange={e => update({ balance_minutes:Number(e.target.value) })} /></label></div>
      <label>余额接口<select aria-label="余额接口" value={draft.balance_adapter} onChange={e => update({ balance_adapter:e.target.value })}><option value="auto">自动识别只读余额接口</option><option value="deepseek">DeepSeek 余额</option><option value="off">关闭</option><option value="openai_usage">只读 /v1/usage</option><option value="new_api_budget">New API 预算（非现金余额）</option></select></label>
      <p className="pool-note">只向此来源请求目录和已选只读接口，不发送聊天、不执行工具、不刷新 OAuth。限流后等待再试。</p>
      <div className="probe-status">{['models','balance'].map(kind => { const s=probes[`${row.id}:${kind}`]; return <p key={kind}><span>{kind === 'models' ? '模型探针' : '余额探针'}</span> {s ? <><code>{s.status}</code> · {new Date(s.last_at*1000).toLocaleTimeString()} · <small>下次 {new Date(s.next_at*1000).toLocaleTimeString()}</small></> : '尚未探测'}</p>; })}</div>
    </fieldset>}
    <footer><button type="submit" disabled={busy || !dirty}>保存配置</button>{row.probe_supported && <button type="button" disabled={busy || dirty} onClick={() => void run(true)}>立即探测</button>}{saved && <span role="status">已保存</span>}{dirty && <small>有未保存更改</small>}</footer>
  </form>;
}
