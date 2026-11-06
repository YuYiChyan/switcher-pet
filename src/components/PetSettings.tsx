import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';
import { petFamilies, PetActivity, usePetAudio, usePetPreferences } from './petPreferences';
import './PetSettings.css';
import './PetArt.css';

export function PetSettings() {
  const {preferences:p,save,error}=usePetPreferences();
  const audio=usePetAudio(p.volume,p.muted);
  const [activity,setActivity]=useState<PetActivity|null>(null);
  const [notice,setNotice]=useState('');
  useEffect(()=>{
    void invoke<PetActivity>('get_pet_activity').then(setActivity);
    const sub=listen<PetActivity>('pet-activity',e=>setActivity(e.payload));
    return()=>{void sub.then(fn=>fn());};
  },[]);
  const importSound=async(which:string)=>{
    try {
      const path=await open({multiple:false,filters:[{name:'MP3 / WAV',extensions:['mp3','wav']}]});
      if(path){await invoke('import_pet_sound',{which,path});setNotice('音效已保存到自用配置，不随安装包分发');}
    }catch(e){setNotice(String(e));}
  };
  const skin=p.follow_activity ? petFamilies.find(f=>activity?.model.toLowerCase().includes(f))||p.fixed_family : p.fixed_family;
  return <section className="pet-settings">
    <header><h2>桌宠与声音</h2><p>日常只显示桌宠，右键打开设置。所有开关立即保存。</p></header>
    <div className="pet-settings-layout"><aside className="pet-settings-preview">
      <img className={`pet-art pet-art-${skin}`} src={`/pets/${skin}.webp`} alt={`${skin} 桌宠预览`} />
      <strong>{activity?.model||'Switcher Pet'}</strong>
      
      <button onClick={()=>void invoke('open_pet').catch(e=>setNotice(String(e)))}>显示桌宠</button>
      <small>fywdy 同人素材，非官方形象</small>
    </aside><div className="pet-settings-controls">
      <fieldset><legend>启动与交互</legend>
        <label><input type="checkbox" checked={p.silent_start} onChange={e=>void save({silent_start:e.target.checked})} />静默启动，只显示桌宠</label>
        <label><input aria-label="捏一捏模式" type="checkbox" checked={p.pinch} onChange={e=>void save({pinch:e.target.checked})} />捏一捏模式</label>
        <p>按住揉捏、松开回弹；拖动桌宠下方的移动按钮调整位置。关闭此模式可直接拖动形象。</p>
        <label><input type="checkbox" checked={p.reduce_motion} onChange={e=>void save({reduce_motion:e.target.checked})} />减少变形动画</label>
      </fieldset>
      <fieldset><legend>自动换装</legend>
        <label><input aria-label="按模型切换形象" type="checkbox" checked={p.follow_activity} onChange={e=>void save({follow_activity:e.target.checked})} />按模型切换形象</label>
        <label>固定 / 等待时形象<select aria-label="固定形象" value={p.fixed_family} onChange={e=>void save({fixed_family:e.target.value})}>{petFamilies.map(f=><option key={f}>{f}</option>)}</select></label>
        <p>只换桌宠形象，不自动更换模型。账号自动切换仍在“模型与来源”内配置，只切同模型。</p>
      </fieldset>
      <fieldset><legend>小黄鸭音效</legend>
        <label><input type="checkbox" checked={p.muted} onChange={e=>void save({muted:e.target.checked})} />静音</label>
        <label>音量<input aria-label="音量" type="range" min="0" max="1" step="0.05" value={p.volume} onChange={e=>void save({volume:Number(e.target.value)})} /><span>{Math.round(p.volume*100)}%</span></label>
        <p>{audio.ready?'已载入按下 / 松开音效':'未配置音效；不会用合成音冒充小黄鸭'}</p>
        <div className="pet-sound-buttons"><button onClick={()=>void importSound('press')}>选择按下音效</button><button onClick={()=>void importSound('release')}>选择松开音效</button><button disabled={!audio.ready||p.muted} onClick={()=>{audio.play('press');setTimeout(()=>audio.play('release'),220);}}>试听小黄鸭</button></div>
        <p>支持本地 MP3 / WAV。自用音效单独保存，安装包不附带许可不明的素材。</p>
      </fieldset>
      
      {(error||audio.error||notice)&&<p role="status">{error||audio.error||notice}</p>}
    </div></div>
  </section>;
}
