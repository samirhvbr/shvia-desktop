'use strict';
const fs=require('node:fs'),path=require('node:path'),http=require('node:http'),https=require('node:https'),net=require('node:net');
const dir=process.env.RUN_DIR||'/fixture',host=process.env.ORIGIN_HOST;
const origin='https://'+host+':8443';
const otherHost=process.env.ORIGIN_OTHER_HOST||'127.0.0.2';
if(!/^[a-zA-Z0-9.-]+$/.test(otherHost))throw Error('Invalid fixture hostname');
const targets={approved:origin,port:'https://'+host+':9443',scheme:'http://'+host+':8443',host:'https://'+otherHost+':8443'};
const cases=[];for(const method of ['blank','open'])for(const destination of ['approved','port','scheme','host'])cases.push({id:destination+'-'+method,method,destination,url:targets[destination]+'/popup?case='+destination+'-'+method});
const state={startedAt:new Date().toISOString(),origin,targets,cases,requests:[],reports:[],gestures:[],commands:[]};
const cert={key:fs.readFileSync(path.join(dir,'server.key')),cert:fs.readFileSync(path.join(dir,'server.crt'))};
function save(){fs.writeFileSync(path.join(dir,'server-state.json'),JSON.stringify(state,null,2));}
function json(res,code,data){res.writeHead(code,{'content-type':'application/json','cache-control':'no-store',connection:'close'});res.end(code===204?'':JSON.stringify(data));}
async function body(req){let v='';for await(const c of req){v+=c;if(v.length>16384)throw Error('oversized body');}return JSON.parse(v||'{}');}
function browser(config){
 const delay=ms=>new Promise(r=>setTimeout(r,ms));
 async function send(kind,extra={}){await fetch('/report',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({kind,case:config.testCase,origin:location.origin,top:window===window.top,...extra})});}
 async function probe(kind){let native=null,error=null;const hasBridge=typeof window.__shviaCode==='object';if(hasBridge)try{const v=await Promise.race([window.__shviaCode.engineStatus('gateway'),delay(12000).then(()=>{throw Error('native timeout');})]);native={found:v.found===true,bundled:v.bundled===true,version:v.version||null};}catch(e){error=String(e.message||e);}
 const buttons={};for(const el of document.querySelectorAll('[data-test]')){const b=el.getBoundingClientRect();buttons[el.dataset.test]={x:b.x,y:b.y,width:b.width,height:b.height};}
 await send(kind,{hasBridge,hasDesktopFlag:!!window.__shviaDesktop,hasOpener:window.opener!==null,native,error,buttons,viewport:{width:innerWidth,height:innerHeight}});}
 // Anchors have no onclick handler. This PASSIVE listener cannot cancel navigation,
 // has no return-false convention, and logs defaultPrevented before and after completed dispatch (setTimeout0, not a microtask).
 document.addEventListener('click',event=>{const el=event.target.closest('[data-test]');if(!el)return;const initial={mode:el.dataset.test,case:config.testCase,isTrusted:event.isTrusted,defaultPreventedBefore:event.defaultPrevented,target:el.target||null,href:el.href||null,userActivation:navigator.userActivation?navigator.userActivation.isActive:null};
 fetch('/gesture',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(initial)});
 setTimeout(()=>send('event-default',{text:JSON.stringify({...initial,defaultPreventedAfter:event.defaultPrevented})}),0);
 },{capture:true,passive:true});
 for(const el of document.querySelectorAll('button[data-open]'))el.addEventListener('click',()=>{
  // Must be synchronous inside the REAL user gesture. Do not await logging/fetch.
  const child=window.open(el.dataset.open,'_blank','width=100,height=100,left=0,top=0,toolbar=no');
  send('window-open-return',{text:JSON.stringify({mode:el.dataset.test,returnedNull:child===null})});
 });
 (async()=>{if(config.nativeExpected){const end=Date.now()+18000;while(typeof window.__shviaCode!=='object'&&Date.now()<end)await delay(30);}else await delay(1500);await probe('ready');let last=0;for(;;){const r=await fetch('/control?case='+encodeURIComponent(config.testCase)+'&after='+last,{cache:'no-store'});for(const c of (await r.json()).commands){last=c.seq;await probe('probe-'+c.seq);}await delay(150);}})().catch(e=>send('page-error',{error:String(e.message||e)}));
}
function page(url,role){const testCase=url.searchParams.get('case')||'main';let controls='';
 if(testCase==='main')for(const c of cases){if(c.method==='blank')controls+='<a data-test="'+c.id+'" href="'+c.url+'" target="_blank">'+c.id+'</a>';else controls+='<button data-test="'+c.id+'" data-open="'+c.url+'">'+c.id+' (100×100 requested)</button>';}
 else { for(const k of ['port','scheme','host'])controls+='<a data-test="child-'+k+'" href="'+targets[k]+'/from-child?case='+testCase+'&kind='+k+'">Child → other '+k+'</a>';
 controls+='<a data-test="nested-blank" href="'+origin+'/popup?case='+testCase+'-nested-blank" target="_blank">Nested target=_blank</a><button data-test="nested-open" data-open="'+origin+'/popup?case='+testCase+'-nested-open">Nested window.open (100×100 requested)</button>'; }
 const conf={testCase,nativeExpected:role==='approved'};
 return '<!doctype html><html><head><meta charset="utf-8"><title>Popup regression '+testCase+'</title><style>body{margin:0;background:#101820;color:#f3f6fb;font:18px system-ui}h1{margin:18px 24px}nav{position:absolute;top:90px;left:24px;display:grid;gap:8px}a,button{box-sizing:border-box;width:430px;height:46px;display:block;background:#dce8fa;color:#102030;border:1px solid #6687b5;padding:10px;text-align:center;font:17px system-ui}pre{position:absolute;left:480px;top:100px;width:560px;white-space:pre-wrap}</style></head><body><h1>Popup regression: '+testCase+'</h1><nav>'+controls+'</nav><pre>Fixture only: real click, no account or user files.\nA link has no onclick. Passive observer never prevents its default action.</pre><script>('+browser.toString()+')('+JSON.stringify(conf)+')</script></body></html>';
}
function handler(scheme){return async(req,res)=>{try{const received=scheme+'://'+req.headers.host;const role=received===origin?'approved':'unapproved';const u=new URL(req.url,received);
 if(!['/control','/status'].includes(u.pathname)){state.requests.push({at:new Date().toISOString(),role,origin:received,method:req.method,path:u.pathname,query:u.search});save();}
 if(u.pathname==='/status'){state.externalOpens=fs.readFileSync(path.join(dir,'external-open.log'),'utf8').split(/\r?\n/).filter(Boolean);return json(res,200,{...state,probeReceivedOrigin:received,probeRole:role});}
 if(u.pathname==='/api/v1/version')return json(res,200,{version:'2.114.180',clients:{desktop:{min_version:'1.0.0',latest_version:process.env.APP_VERSION||'1.42.0'}}});
 if(u.pathname==='/api/v1/health')return json(res,200,{ok:true});
 if(u.pathname.startsWith('/api/v1/desktop/update/'))return json(res,204,null);
 if(u.pathname==='/api/v1/notifications')return json(res,200,[]);
 if(u.pathname==='/report'&&req.method==='POST'){const d=await body(req);if(Object.keys(d).some(k=>!['kind','case','origin','top','hasBridge','hasDesktopFlag','hasOpener','native','error','buttons','viewport','text'].includes(k)))return json(res,400,{error:'report field'});state.reports.push({...d,role,at:new Date().toISOString()});save();return json(res,200,{ok:true});}
 if(u.pathname==='/gesture'&&req.method==='POST'){const d=await body(req);if(Object.keys(d).some(k=>!['mode','case','isTrusted','defaultPreventedBefore','target','href','userActivation'].includes(k)))return json(res,400,{error:'gesture field'});state.gestures.push({...d,role,at:new Date().toISOString()});save();return json(res,200,{ok:true});}
 if(u.pathname==='/control')return json(res,200,{commands:state.commands.filter(c=>c.seq>Number(u.searchParams.get('after')||0)&&c.target===u.searchParams.get('case'))});
 if(u.pathname==='/command'&&req.method==='POST'){const d=await body(req),c={seq:state.commands.length+1,target:d.target};state.commands.push(c);save();return json(res,200,c);}
 if(req.method!=='GET')return json(res,405,{error:'method'});
 res.writeHead(200,{'content-type':'text/html;charset=utf-8','cache-control':'no-store'});res.end(page(u,role));
 }catch(e){json(res,500,{error:e.message});}};}
// Multiplex HTTP and TLS on8443 so the scheme case changes ONLY the scheme,
// while both possible documents genuinely exist if the native policy fails.
const plain=http.createServer(handler('http')),tls=https.createServer(cert,handler('https'));
plain.listen(18080,'127.0.0.1');tls.listen(18443,'127.0.0.1');
const front=net.createServer(socket=>{socket.on('error',()=>socket.destroy());socket.once('data',chunk=>{socket.pause();const backend=net.createConnection({host:'127.0.0.1',port:chunk[0]===22?18443:18080},()=>{backend.write(chunk);socket.pipe(backend).pipe(socket);socket.resume();});backend.on('error',()=>socket.destroy());socket.on('error',()=>backend.destroy());socket.on('close',()=>backend.destroy());});});
front.listen(8443,'0.0.0.0');https.createServer(cert,handler('https')).listen(9443,'0.0.0.0');save();console.log(JSON.stringify({ready:true,origin,targets}));
