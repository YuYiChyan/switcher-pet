import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import './DesktopPet.css';
import { petFamilies as families, PetActivity as Activity, usePetPreferences, usePetAudio } from './petPreferences';
import { choosePhrase, formatBalance, windowPercent, quotaText, widgetBalances, balancePercent, widgetQuotaWindows } from './petWidget';
type Quota = { remaining:number; capacity:number; known:number; unknown:number; stale?:number };
type Summary = { members:number; show_short:boolean; short:Quota; weekly:Quota; balances:Record<string,number>; balance_capacities:Record<string,number>;balance_estimated:Record<string,boolean> };
type Pool = { summary:Summary; model:string; model_summary:Summary; model_quota:Quota; model_short:Quota;model_weekly:Quota;accounts:{ id:string; summary:Summary }[] };
function family(model:string) { return families.find(f=>model.toLowerCase().includes(f)) ?? ''; }
function Meter({label,value,percent:provided,estimated=false,compact=false,displayText}:{label:string;value?:Quota;percent?:number|null;estimated?:boolean;compact?:boolean;displayText?:string}) {
  const percent=provided!==undefined?provided:value?windowPercent(value):null;
  const percentage=value?quotaText(value):percent===null?'—':`${estimated?'≈':''}${Math.round(percent)}%`;
  const text=displayText??percentage;
  const hint=value?`${text} / ≈${Math.round(value.capacity)}% · 总池剩余${percent===null?'未知':Math.round(percent)+'%'}${value.unknown?' · 部分账号尚无额度':''}${value.stale?' · 含上次记录':''}`:displayText?`${displayText} · ${percentage}`:text;
  return <div className={`pet-meter ${compact?'is-mini':''}`} title={hint}><div><span>{label}</span><span>{text}{value&&!compact&&value.capacity>0?` / ≈${Math.round(value.capacity)}%`:''}</span></div>
    <div className={`pet-galaxy-track ${percent===null?'is-unknown':''}`} role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent??undefined} aria-valuetext={hint}>
      <span className="pet-galaxy-fill" style={{width:`${percent??0}%`}}><i/><b/></span>
    </div>
  </div>;
}
export function DesktopPet() {
  const [activity,setActivity]=useState<Activity>({model:'',source:'',account_id:'',at:0,session_id:'',origin:''});
  const [pool,setPool]=useState<Pool|null>(null);
  const [phrase,setPhrase]=useState('');
  const previousPhrase=useRef('');
  const bubbleTimer=useRef<ReturnType<typeof setTimeout>|undefined>(undefined);
  const {preferences:p,save,error}=usePetPreferences();
  const open=p.detailed;
  const audio=usePetAudio(p.volume,p.muted);
  const [squash,setSquash]=useState({x:1,y:1,held:false});
  const prefRef=useRef(p);prefRef.current=p;
  const pointer=useRef<{x:number;y:number;dragged:boolean}|null>(null);
  const hold=useRef<{timer?:ReturnType<typeof setTimeout>;x:number;y:number;fired:boolean}>({x:0,y:0,fired:false});
  const [notice,setNotice]=useState('');
  const [showError,setShowError]=useState(false);
  const requestSequence=useRef(0);
  const refresh=async()=>{const sequence=++requestSequence.current;try{const next=await invoke<Pool>('get_pool_dashboard');if(sequence===requestSequence.current)setPool(next);}catch(e){setNotice(String(e));}};
  const setMuted=()=>void save({muted:!prefRef.current.muted});
  const toggleMode=()=>void save({detailed:!prefRef.current.detailed});
  const cancelHold=()=>{clearTimeout(hold.current.timer);};
  const startHold=(x:number,y:number)=>{cancelHold();hold.current={x,y,fired:false,timer:setTimeout(()=>{hold.current.fired=true;toggleMode();},650)};};
  const moveHold=(x:number,y:number)=>{if(Math.hypot(x-hold.current.x,y-hold.current.y)>6)cancelHold();};
  const showPhrase=()=>{const text=choosePhrase(previousPhrase.current);previousPhrase.current=text;setPhrase(text);clearTimeout(bubbleTimer.current);bubbleTimer.current=setTimeout(()=>setPhrase(''),4500);};
  useEffect(()=>{
    document.documentElement.classList.add('pet-surface');document.body.classList.add('pet-surface');
    void refresh();void invoke<Activity>('get_pet_activity').then(setActivity);
    const subscriptions=[listen<Activity>('pet-activity',e=>{setActivity(e.payload);void refresh();}),listen('accounts-updated',()=>void refresh()),
      listen('pet-toggle-mute',setMuted),listen('pet-toggle-pool',()=>{toggleMode();void refresh();}),
      listen('pet-route-selected',()=>void refresh()),listen<string>('pet-error',e=>{setNotice(e.payload);setShowError(true);})];
    const timer=setInterval(()=>void refresh(),15000);
    return()=>{clearInterval(timer);clearTimeout(bubbleTimer.current);cancelHold();subscriptions.forEach(s=>void s.then(fn=>fn()));};
  },[]);
  useEffect(()=>{const expanded=open||!!phrase;const resize=()=>void invoke('resize_pet',{expanded}).catch(e=>setNotice(String(e)));
    if(expanded||p.reduce_motion){resize();return;}const timer=setTimeout(resize,380);return()=>clearTimeout(timer);},[open,!!phrase,p.reduce_motion]);
  const summary=pool?.model_summary;
  const total=pool?.summary;
  const {short,quota,label:quotaLabel}=widgetQuotaWindows(total,summary,pool?.model_weekly,pool?.model_quota,pool?.model_short);
  const balances=summary?widgetBalances(summary):[];
  const shown=p.follow_activity?(activity.model?family(activity.model):p.fixed_family):p.fixed_family;
  const model=pool?.model||activity.model.replace(/^relay-current:/,'')||'Switcher Pet';
  const release=()=>{audio.play('release');setSquash({x:1,y:1,held:false});};
  const problem=notice||error||audio.error;
  return <main className={`desktop-pet ${p.reduce_motion?'motion-off':''}`} onContextMenu={e=>{e.preventDefault();cancelHold();void invoke('pet_menu').catch(e=>setNotice(String(e)));}}>
    {phrase && <button className="pet-speech" aria-label="捏一捏对话" onClick={showPhrase}>{phrase}</button>}
    <button className={`pet-character ${squash.held?'pet-held':''}`} aria-label="查看用量"
      style={{transform:`scale(${squash.x},${squash.y})`,transition:squash.held||p.reduce_motion?'none':undefined}}
      onPointerDown={e=>{if(e.button!==0)return;startHold(e.screenX,e.screenY);pointer.current={x:e.screenX,y:e.screenY,dragged:false};
        if(p.pinch){e.currentTarget.setPointerCapture(e.pointerId);setSquash({x:p.reduce_motion?1:1.12,y:p.reduce_motion?1:.86,held:true});audio.play('press');}}}
      onPointerMove={e=>{moveHold(e.screenX,e.screenY);const start=pointer.current;if(!start)return;const distance=Math.hypot(e.screenX-start.x,e.screenY-start.y);
        if(p.pinch){if(!p.reduce_motion){const squeeze=Math.max(-.2,Math.min(.3,(e.screenY-start.y)/180));setSquash({x:1.12+squeeze,y:.86-squeeze,held:true});}}
        else if(!start.dragged&&distance>6){start.dragged=true;void invoke('drag_pet').catch(e=>setNotice(String(e)));}}}
      onPointerUp={e=>{cancelHold();if(p.pinch){release();if(e.currentTarget.hasPointerCapture(e.pointerId))e.currentTarget.releasePointerCapture(e.pointerId);}
        else if(!pointer.current?.dragged){audio.play('press');setTimeout(()=>audio.play('release'),160);}}}
      onPointerCancel={()=>{cancelHold();setSquash({x:1,y:1,held:false});pointer.current=null;}}
      onLostPointerCapture={()=>{cancelHold();setSquash({x:1,y:1,held:false});}}
      onClick={e=>{if(hold.current.fired){hold.current.fired=false;pointer.current=null;return;}if(!pointer.current?.dragged){if(p.pinch){showPhrase();}else toggleMode();void refresh();
        if(e.detail===0){audio.play('press');setTimeout(()=>audio.play('release'),160);}}pointer.current=null;}}>
      {shown?<img src={`/pets/${shown}.webp`} draggable={false} alt="" />:<span className="pet-generic">✦</span>}
    </button>
    <section className={`pet-widget ${open?'is-expanded':''}`} aria-label="桌宠用量">
      <header><button className="pet-model" title={model} aria-label="切换详细模式" aria-expanded={open}
        onPointerDown={e=>{if(e.button===0){startHold(e.screenX,e.screenY);e.currentTarget.setPointerCapture(e.pointerId);}}}
        onPointerMove={e=>moveHold(e.screenX,e.screenY)} onPointerUp={cancelHold} onPointerCancel={cancelHold} onLostPointerCapture={cancelHold}
        onClick={()=>{if(hold.current.fired){hold.current.fired=false;return;}toggleMode();void refresh();}}><span className="pet-model-name">{model}</span>
        <span className="pet-mini" aria-hidden={open}>
          {short&&<Meter compact label="五小时" value={short}/>}
          {!!quota?.capacity&&<Meter compact label={quotaLabel} value={quota}/>}
          {balances.map(b=><Meter key={b.unit} compact label={balances.length>1?b.unit:'API'} displayText={formatBalance(b.unit,b.amount)} percent={balancePercent(b.amount,b.capacity)} estimated={b.estimated}/>)}
          {!balances.length&&!quota?.capacity&&<Meter compact label={quotaLabel} percent={null}/>}
        </span>
      </button>
        <button className="pet-move" aria-label="移动桌宠" title="拖动移动桌宠" onPointerDown={e=>{if(e.button===0)void invoke('drag_pet');}}>⠿</button></header>
      <div className="pet-widget-reveal" aria-hidden={!open}><div className="pet-widget-clip"><div className="pet-widget-body">
        {balances.length ? balances.map(({unit,amount,capacity,estimated})=>{
          const percent=balancePercent(amount,capacity);
          const amountText=formatBalance(unit,amount);
          return <div className="pet-api-balance" key={unit}><div className="pet-balance"><strong className="pet-balance-amount" style={{fontSize:amountText.length>17?18:amountText.length>12?22:undefined}}>{amountText}</strong><span>{percent!==null?`${estimated?'≈':''}${Math.round(percent)}%`:'—%'}</span></div><Meter label={balances.length>1?unit:'API'} displayText={amountText} percent={percent} estimated={estimated}/></div>;
        }): <div className="pet-balance"><strong>{quota?.known?`≈${Math.round(quota.remaining)}%`:'—%'}</strong><span>{quota?.known?`${(quota.remaining/100).toLocaleString(undefined,{maximumFractionDigits:2})}×`:'—'}</span></div>}
        {total && <footer className="pet-pool">
          {short&&<Meter label="五小时" value={short}/>}
          {!!quota?.capacity&&<Meter label={quotaLabel} value={quota}/>}
        </footer>}
      </div></div></div>
      {problem && <button className="pet-alert" aria-label="查看错误" onClick={()=>setShowError(v=>!v)}>!</button>}
      {showError&&problem&&<p role="status" className="pet-error">{problem}</p>}
    </section>
  </main>;
}
