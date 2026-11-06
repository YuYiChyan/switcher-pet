// Local squeeze phrases, with the whale widget's no-consecutive-repeat behavior.
const phrases = ['好模型……','好女孩……','没吃饱喵','终于上当了！','不知道用户有什么用，先养着吧～','我……我也要挣钱吗？','我去吃饭啦，测完叫我','再捏就要变成小饼了！','轻一点嘛～','今天也有认真陪你哦'];
export function choosePhrase(previous:string, random=Math.random):string {
  const choices=phrases.filter(line=>line!==previous);
  return choices[Math.min(choices.length-1,Math.max(0,Math.floor(random()*choices.length)))];
}
export function windowPercent(value:{remaining:number;capacity:number;known:number;unknown:number}):number|null {
  if(!value.known||!(value.capacity>0))return null;
  return Math.max(0,Math.min(100,value.remaining/value.capacity*100));
}
export function quotaText(value:{remaining:number;known:number;unknown:number;stale?:number}):string {
  if(!value.known)return '—';
  return `≈${Math.round(value.remaining)}%`;
}
export function formatBalance(unit:string,amount:number):string {
  const currency=unit.split(' / ').pop()||'';
  const number=amount.toLocaleString(undefined,{minimumFractionDigits:2,maximumFractionDigits:2});
  return ({USD:'$',CNY:'¥',EUR:'€'} as Record<string,string>)[currency]?.concat(number) ?? `${number} ${currency}`;
}
// Widget totals are spendable API credit, not a cash valuation. The detailed
// settings ledger continues to distinguish prepaid money from usage budgets.
export function widgetBalances(summary:{balances:Record<string,number>;balance_capacities?:Record<string,number>;balance_estimated?:Record<string,boolean>}) {
  const currencies=new Map<string,{unit:string;amount:number;capacity:number|null;estimated:boolean}>();
  for(const [key,amount] of Object.entries(summary.balances)) {
    const unit=key.split(' / ').pop()||'';
    const capacity=summary.balance_capacities?.[key]??null;
    const old=currencies.get(unit);
    currencies.set(unit,{unit,amount:amount+(old?.amount??0),capacity:old?(old.capacity===null||capacity===null?null:old.capacity+capacity):capacity,
      estimated:!!old?.estimated||!!summary.balance_estimated?.[key]});
  }
  return [...currencies.values()];
}
export function balancePercent(amount:number,capacity:number|null):number|null {
  if(amount<=0)return 0;
  return capacity && capacity>0?Math.max(0,Math.min(100,amount/capacity*100)):null;
}
type QuotaWindow={remaining:number;capacity:number;known:number;unknown:number};
type QuotaSummary={show_short:boolean;short:QuotaWindow;weekly:QuotaWindow};
export function widgetQuotaWindows(total:QuotaSummary|undefined,modelSummary:QuotaSummary|undefined,
  modelWeekly:QuotaWindow|undefined,modelQuota:QuotaWindow|undefined,modelShort?:QuotaWindow) {
  // Pool-level subscription limits are independent of the currently selected
  // model. A model-specific quota must not hide a real five-hour pool window.
  // Conversely, a conclusive pool result with no limited account suppresses
  // model-only short windows, matching the user's "weekly only" preference.
  const short=total
    ? (total.show_short?total.short:undefined)
    : (modelShort?.capacity?modelShort:modelSummary?.show_short?modelSummary.short:undefined);
  if(total?.weekly.capacity)return {short,quota:total.weekly,label:'周额度'};
  if(modelWeekly?.capacity)return {short,quota:modelWeekly,label:'周额度'};
  if(modelQuota?.capacity)return {short,quota:modelQuota,label:'额度'};
  if(modelSummary?.weekly.capacity)return {short,quota:modelSummary.weekly,label:'周额度'};
  return {short,quota:undefined,label:'周额度'};
}
