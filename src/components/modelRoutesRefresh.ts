export function watchModelRoutes(refresh:()=>void,intervalMs=15_000):()=>void {
  let disposed=false;
  const guarded=()=>{if(!disposed)refresh();};
  const timer=window.setInterval(guarded,intervalMs);
  const onVisibility=()=>{if(document.visibilityState==='visible')guarded();};
  document.addEventListener('visibilitychange',onVisibility);
  return ()=>{
    disposed=true;
    window.clearInterval(timer);
    document.removeEventListener('visibilitychange',onVisibility);
  };
}
