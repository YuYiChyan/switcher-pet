import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
export const petFamilies = ['gpt','claude','deepseek','gemini','grok','kimi','qwen','glm'];
export type PetPreferences = { silent_start:boolean; pinch:boolean; follow_activity:boolean; fixed_family:string; muted:boolean; total:boolean; volume:number; reduce_motion:boolean; detailed:boolean };
export type PetActivity = {model:string;source:string;account_id:string;at:number;session_id:string;origin:string};
export function usePetPreferences() {
  const [preferences,setPreferences]=useState<PetPreferences>({silent_start:true,pinch:true,follow_activity:true,fixed_family:'gpt',muted:false,total:true,volume:.5,reduce_motion:false,detailed:false});
  const [error,setError]=useState('');
  const ref=useRef(preferences);
  const accept=(p:PetPreferences)=>{ref.current=p;setPreferences(p);};
  useEffect(()=>{
    void invoke<PetPreferences>('get_pet_preferences').then(accept).catch(e=>setError(String(e)));
    const sub=listen<PetPreferences>('pet-preferences',e=>accept(e.payload));
    return()=>{void sub.then(fn=>fn());};
  },[]);
  const save=async(patch:Partial<PetPreferences>)=>{
    const next={...ref.current,...patch};accept(next);setError('');
    try{await invoke('set_pet_preferences',{preferences:next});}catch(e){setError(String(e));void invoke<PetPreferences>('get_pet_preferences').then(accept);}
  };
  return {preferences,save,error};
}
export function usePetAudio(volume:number,muted:boolean) {
  const sounds=useRef<Record<string,HTMLAudioElement>>({});
  const [ready,setReady]=useState(false);
  const [error,setError]=useState('');
  useEffect(()=>{
    let disposed=false;
    const load=async()=>{
      try {
        const [press,release]=await Promise.all(['press','release'].map(which=>invoke<string|null>('get_pet_sound',{which})));
        if(disposed)return;
        Object.values(sounds.current).forEach(a=>a.pause());
        sounds.current={};if(press)sounds.current.press=new Audio(press);if(release)sounds.current.release=new Audio(release);
        setReady(!!press && !!release);setError('');
      }catch(e){setError(String(e));}
    };
    void load();const sub=listen('pet-sounds-updated',()=>void load());
    return()=>{disposed=true;Object.values(sounds.current).forEach(a=>a.pause());void sub.then(fn=>fn());};
  },[]);
  const play=(which:'press'|'release')=>{
    if(muted)return;const a=sounds.current[which];if(!a)return;
    a.volume=volume;a.currentTime=0;void a.play().catch(e=>setError(String(e)));
  };
  return {ready,error,play};
}
