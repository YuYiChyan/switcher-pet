import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import ts from 'typescript';
const code=ts.transpile(fs.readFileSync('src/components/petWidget.ts','utf8'),{module:ts.ModuleKind.ES2022,target:ts.ScriptTarget.ES2022});
const {choosePhrase,windowPercent,quotaText,formatBalance,widgetBalances,balancePercent,widgetQuotaWindows}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const refreshCode=ts.transpile(fs.readFileSync('src/components/modelRoutesRefresh.ts','utf8'),{module:ts.ModuleKind.ES2022,target:ts.ScriptTarget.ES2022});
const {watchModelRoutes}=await import('data:text/javascript;base64,'+Buffer.from(refreshCode).toString('base64'));
test('daily credit is one total per currency without inventing missing capacity',()=>{
 assert.deepEqual(widgetBalances({balances:{'balance / USD':12.5,'budget / USD':48,'balance / CNY':3},balance_capacities:{'budget / USD':50}}),[
 {unit:'USD',amount:60.5,capacity:null,estimated:false},{unit:'CNY',amount:3,capacity:null,estimated:false}]);
});
test('API zero remains 0 percent; reference tracks consumption instead of always 100',()=>{
 assert.equal(balancePercent(0,null),0);assert.equal(balancePercent(8,10),80);
 const [row]=widgetBalances({balances:{'balance / USD':8},balance_capacities:{'balance / USD':10},balance_estimated:{'balance / USD':true}});
 assert.equal(row.estimated,true);assert.equal(balancePercent(row.amount,row.capacity),80);
});
test('squeeze phrases never repeat consecutively, including repeated random samples',()=>{
 let last='';for(let i=0;i<30;i++){const next=choosePhrase(last,()=>0);assert.notEqual(next,last);last=next;}
});
test('pool bar uses total capacity rather than Plus-equivalent percent',()=>{
 assert.equal(windowPercent({remaining:500,capacity:1000,known:2,unknown:0}),50);
 assert.equal(windowPercent({remaining:500,capacity:1000,known:2,unknown:1}),50);
 assert.equal(windowPercent({remaining:0,capacity:1000,known:2,unknown:0}),0);
});
test('quota number stacks beyond 100 while a partial pool keeps its bar',()=>{
 const value={remaining:500,capacity:1000,known:2,unknown:1,stale:1};
 assert.equal(quotaText(value),'≈500%');assert.equal(windowPercent(value),50);
 assert.equal(quotaText({...value,unknown:0}),'≈500%');
 assert.equal(quotaText({...value,known:0}),'—');
});
test('pool short and weekly windows stay visible when the active model has a separate quota',()=>{
 const total={show_short:true,short:{remaining:60,capacity:100,known:1,unknown:0},weekly:{remaining:420,capacity:1000,known:3,unknown:0}};
 const modelSummary={show_short:false,short:{remaining:0,capacity:0,known:0,unknown:0},weekly:{remaining:0,capacity:0,known:0,unknown:0}};
 const modelWeekly={remaining:90,capacity:100,known:1,unknown:0};
 const modelQuota={remaining:80,capacity:100,known:1,unknown:0};
 assert.deepEqual(widgetQuotaWindows(total,modelSummary,modelWeekly,modelQuota),{
  short:total.short,quota:total.weekly,label:'周额度'
 });
});
test('conclusive pool without a five-hour limit hides model-only short windows',()=>{
 const total={show_short:false,short:{remaining:0,capacity:0,known:0,unknown:0},weekly:{remaining:500,capacity:1000,known:2,unknown:0}};
 const modelSummary={show_short:true,short:{remaining:20,capacity:100,known:1,unknown:0},weekly:{remaining:0,capacity:0,known:0,unknown:0}};
 assert.deepEqual(widgetQuotaWindows(total,modelSummary,{remaining:0,capacity:0,known:0,unknown:0},{remaining:0,capacity:0,known:0,unknown:0}),{
  short:undefined,quota:total.weekly,label:'周额度'
 });
});
test('balance is currency plus number, without provider labels',()=>{
 assert.equal(formatBalance('budget / USD',48),'$48.00');
 assert.equal(formatBalance('balance / CNY',0),'¥0.00');
});
test('pet contains no account source, session id or technical observer captions',()=>{
 const s=fs.readFileSync('src/components/DesktopPet.tsx','utf8');
 assert.doesNotMatch(s,/\{activity\.(source|session_id)|只读观察|活跃对话|非窗口焦点|未知余额来源/);
 assert.match(s,/model_summary/);assert.match(s,/<footer className="pet-pool"/);
});
test('model route refresh reacts to polling and returning to the visible app',()=>{
 const savedWindow=globalThis.window,savedDocument=globalThis.document;
 let tick,visibility,period,cleared=false,removed=false,calls=0;
 globalThis.window={setInterval:(fn,ms)=>{tick=fn;period=ms;return 7;},clearInterval:id=>{assert.equal(id,7);cleared=true;}};
 globalThis.document={visibilityState:'hidden',addEventListener:(name,fn)=>{assert.equal(name,'visibilitychange');visibility=fn;},
  removeEventListener:(name,fn)=>{assert.equal(name,'visibilitychange');assert.equal(fn,visibility);removed=true;}};
 try{
  const stop=watchModelRoutes(()=>calls++);
  tick();assert.equal(calls,1);assert.equal(period,15000);
  visibility();assert.equal(calls,1);
  document.visibilityState='visible';visibility();assert.equal(calls,2);
  stop();assert.ok(cleared&&removed);
  visibility();assert.equal(calls,2);
 }finally{
  if(savedWindow===undefined)delete globalThis.window;else globalThis.window=savedWindow;
  if(savedDocument===undefined)delete globalThis.document;else globalThis.document=savedDocument;
 }
});
