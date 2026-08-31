import fs from 'node:fs';
import path from 'node:path';

const OUT = path.resolve('docs/ui-redesign');
const W = 1600;
const H = 1000;
const C = {
  bg:'#F7F7F5', side:'#F0F0EE', paper:'#FFFFFF', pane:'#FBFBFA', ink:'#20211F', text:'#383A36',
  muted:'#747770', faint:'#A4A79F', line:'#DADCD7', line2:'#EAEBE7', orange:'#F06445', orangeSoft:'#FBE9E4',
  green:'#23806D', greenSoft:'#E6F1ED', blue:'#5E6AD2', blueSoft:'#EDEEFE', purple:'#8465BC', purpleSoft:'#F0EBF7',
  yellow:'#A97928', yellowSoft:'#F5EEDC', terminal:'#20231F', terminal2:'#292D27', terminalText:'#DDE2D9'
};
const sans=`Inter, -apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", "Microsoft YaHei", sans-serif`;
const mono=`SFMono-Regular, Menlo, Monaco, Consolas, monospace`;
const esc=s=>String(s).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');
const at=a=>Object.entries(a).filter(([,v])=>v!==undefined).map(([k,v])=>`${k.replace(/[A-Z]/g,m=>'-'+m.toLowerCase())}="${esc(v)}"`).join(' ');
const rect=(x,y,w,h,fill=C.paper,stroke='none',rx=0,e={})=>`<rect ${at({x,y,width:w,height:h,fill,stroke,rx,...e})}/>`;
const line=(x1,y1,x2,y2,stroke=C.line,sw=1,e={})=>`<line ${at({x1,y1,x2,y2,stroke,strokeWidth:sw,...e})}/>`;
const circle=(cx,cy,r,fill,stroke='none',sw=1)=>`<circle ${at({cx,cy,r,fill,stroke,strokeWidth:sw})}/>`;
const pathEl=(d,stroke=C.ink,sw=1.6,fill='none',e={})=>`<path ${at({d,stroke,strokeWidth:sw,fill,strokeLinecap:'round',strokeLinejoin:'round',...e})}/>`;
const txt=(x,y,s,size=14,color=C.ink,weight=500,anchor='start',family=sans,e={})=>`<text ${at({x,y,fontFamily:family,fontSize:size,fill:color,fontWeight:weight,textAnchor:anchor,...e})}>${esc(s)}</text>`;
const rows=(x,y,arr,size=14,color=C.text,gap=22,weight=450,family=sans)=>arr.map((s,i)=>txt(x,y+i*gap,s,size,color,weight,'start',family)).join('');

function defs(){return `<defs><filter id="shadow" x="-30%" y="-30%" width="160%" height="180%"><feDropShadow dx="0" dy="10" stdDeviation="22" flood-color="#20211F" flood-opacity=".10"/></filter><filter id="lift" x="-30%" y="-30%" width="160%" height="180%"><feDropShadow dx="0" dy="4" stdDeviation="10" flood-color="#20211F" flood-opacity=".08"/></filter><pattern id="graphDots" width="20" height="20" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r="1" fill="#BFC2BB" fill-opacity=".35"/></pattern><marker id="graphArrow" markerWidth="9" markerHeight="9" refX="8" refY="4.5" orient="auto"><path d="M0 0L9 4.5L0 9Z" fill="context-stroke"/></marker><marker id="graphArrowContext" markerWidth="8" markerHeight="8" refX="7" refY="4" orient="auto"><path d="M0 0L8 4L0 8Z" fill="context-stroke"/></marker></defs>`}
function doc(title,body,bg=C.bg){return `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}"><title>${esc(title)}</title>${defs()}${rect(0,0,W,H,bg)}${body}</svg>`}
function icon(name,x,y,s=20,color=C.muted,sw=1.6){
  const q=s/20,X=n=>x+n*q,Y=n=>y+n*q,p=d=>pathEl(d,color,sw);
  const d={
    new:`M${X(10)} ${Y(3)}V${Y(17)}M${X(3)} ${Y(10)}H${X(17)}`,
    chat:`M${X(3)} ${Y(4)}H${X(17)}V${Y(14)}H${X(9)}L${X(5)} ${Y(17)}V${Y(14)}H${X(3)}Z`,
    folder:`M${X(2)} ${Y(5)}H${X(8)}L${X(10)} ${Y(7)}H${X(18)}V${Y(16)}H${X(2)}Z`,
    search:`M${X(13)} ${Y(13)}L${X(18)} ${Y(18)}`,
    clock:`M${X(10)} ${Y(4)}V${Y(10)}L${X(14)} ${Y(12)}`,
    apps:`M${X(3)} ${Y(3)}H${X(8)}V${Y(8)}H${X(3)}ZM${X(12)} ${Y(3)}H${X(17)}V${Y(8)}H${X(12)}ZM${X(3)} ${Y(12)}H${X(8)}V${Y(17)}H${X(3)}ZM${X(12)} ${Y(12)}H${X(17)}V${Y(17)}H${X(12)}Z`,
    settings:`M${X(10)} ${Y(2)}V${Y(5)}M${X(10)} ${Y(15)}V${Y(18)}M${X(2)} ${Y(10)}H${X(5)}M${X(15)} ${Y(10)}H${X(18)}`,
    terminal:`M${X(3)} ${Y(4)}H${X(17)}V${Y(16)}H${X(3)}ZM${X(6)} ${Y(8)}L${X(9)} ${Y(10)}L${X(6)} ${Y(12)}M${X(11)} ${Y(13)}H${X(14)}`,
    file:`M${X(5)} ${Y(2)}H${X(12)}L${X(16)} ${Y(6)}V${Y(18)}H${X(5)}ZM${X(12)} ${Y(2)}V${Y(6)}H${X(16)}`,
    plan:`M${X(5)} ${Y(3)}H${X(16)}V${Y(17)}H${X(5)}ZM${X(8)} ${Y(7)}H${X(13)}M${X(8)} ${Y(10)}H${X(13)}M${X(8)} ${Y(13)}H${X(12)}`,
    note:`M${X(4)} ${Y(3)}H${X(16)}V${Y(17)}H${X(4)}ZM${X(7)} ${Y(7)}H${X(13)}M${X(7)} ${Y(10)}H${X(13)}M${X(7)} ${Y(13)}H${X(11)}`,
    branch:`M${X(6)} ${Y(4)}V${Y(16)}M${X(6)} ${Y(9)}H${X(12)}Q${X(15)} ${Y(9)} ${X(15)} ${Y(6)}`,
    send:`M${X(3)} ${Y(10)}L${X(17)} ${Y(3)}L${X(12)} ${Y(17)}L${X(9)} ${Y(11)}Z`,
    chevron:`M${X(7)} ${Y(4)}L${X(13)} ${Y(10)}L${X(7)} ${Y(16)}`,
    spark:`M${X(10)} ${Y(2)}L${X(12)} ${Y(8)}L${X(18)} ${Y(10)}L${X(12)} ${Y(12)}L${X(10)} ${Y(18)}L${X(8)} ${Y(12)}L${X(2)} ${Y(10)}L${X(8)} ${Y(8)}Z`
  };
  if(name==='search')return circle(X(8),Y(8),5*q,'none',color,sw)+p(d.search);
  if(name==='clock')return circle(X(10),Y(10),7*q,'none',color,sw)+p(d.clock);
  if(name==='settings')return circle(X(10),Y(10),5*q,'none',color,sw)+circle(X(10),Y(10),2*q,'none',color,sw)+p(d.settings);
  return p(d[name]||d.file);
}
function logo(x,y,size=32,word=true){let o=rect(x,y,size,size,C.ink,'none',9);o+=pathEl(`M${x+7} ${y+size*.5}C${x+11} ${y+7},${x+size-11} ${y+7},${x+size-7} ${y+size*.5}C${x+size-11} ${y+size-7},${x+11} ${y+size-7},${x+7} ${y+size*.5}Z`,'#F7F7F5',1.8);o+=circle(x+size/2,y+size/2,2.5,C.orange);if(word)o+=txt(x+size+10,y+size*.68,'ChatOS',16,C.ink,700);return o}
function chip(x,y,label,color=C.muted,fill=C.side,w){w=w||Math.max(56,label.length*11+22);return rect(x,y,w,26,fill,'none',13)+txt(x+w/2,y+17,label,9,color,650,'middle')}
function button(x,y,label,kind='default',w=92,ico){const bg=kind==='dark'?C.ink:kind==='accent'?C.orange:C.paper,fg=kind==='default'?C.ink:C.paper,bd=kind==='default'?C.line:bg;let o=rect(x,y,w,36,bg,bd,9);if(ico)o+=icon(ico,x+12,y+8,18,fg);o+=txt(x+(ico?38:w/2),y+23,label,11,fg,650,ico?'start':'middle');return o}
function windowBar(){return rect(0,0,W,38,'#FAFAF9',C.line2)+circle(18,19,5,'#DFA39B')+circle(36,19,5,'#E2BD71')+circle(54,19,5,'#83AF96')+txt(W/2,24,'ChatOS',10,C.faint,550,'middle')}

const sessions=[
  ['模型配置页重构','正在等待确认','12m',C.orange],['Task Runner 权限边界','验证通过','1h',C.green],['检查项目启动流程','已完成','昨天',C.faint],['Agent 角色系统','草稿','周一',C.faint]
];
function sidebar(active='模型配置页重构',mode='work'){
  let o=rect(0,38,272,H-38,C.side,'none')+line(272,38,272,H,C.line);
  o+=logo(20,56,32,true)+txt(244,78,'⌄',12,C.muted,650,'end');
  o+=button(16,108,'新建任务','dark',240,'new');
  [['chat','工作','work'],['clock','自动任务','auto'],['apps','应用','apps']].forEach(([ico,label,id],i)=>{const y=160+i*40;if(id===mode)o+=rect(12,y-7,248,34,C.paper,'none',8);o+=icon(ico,25,y,17,id===mode?C.ink:C.muted)+txt(52,y+13,label,11,id===mode?C.ink:C.muted,id===mode?650:520);});
  o+=txt(24,304,'项目',9,C.faint,700)+txt(246,304,'＋',14,C.muted,500,'end');
  [['ChatOS','main'],['Task Runner','feature/runtime'],['Project Manager','main']].forEach(([n,b],i)=>{const y=326+i*37;o+=icon('folder',24,y,15,C.muted)+txt(48,y+12,n,10,C.text,580)+txt(244,y+12,b,7,C.faint,500,'end',mono);});
  o+=txt(24,455,'最近会话',9,C.faint,700);
  sessions.forEach(([name,state,time,color],i)=>{const y=470+i*62;if(name===active)o+=rect(12,y,248,54,C.paper,'none',9);o+=circle(29,y+17,4,color)+txt(43,y+20,name,11,name===active?C.ink:C.text,name===active?650:520)+txt(43,y+39,state,8,C.faint,480)+txt(244,y+39,time,8,C.faint,500,'end');});
  o+=line(16,862,256,862,C.line)+circle(26,887,4,C.green)+txt(40,891,'Local Connector',9,C.text,580)+txt(246,891,'在线',8,C.green,650,'end');
  o+=circle(26,918,4,C.orange)+txt(40,922,'Task Runner',9,C.text,580)+txt(246,922,'1 运行中',8,C.orange,650,'end');
  o+=circle(29,958,15,C.ink)+txt(29,962,'L',9,C.paper,700,'middle')+txt(52,956,'Lee',10,C.ink,650)+txt(52,971,'Workspace owner',7,C.faint,500)+icon('settings',230,949,18,C.muted);
  return o;
}
function topbar(title='模型配置页重构',repo='chatos-rs / main',status='运行中',actions=''){
  let o=rect(272,38,W-272,54,'#FAFAF9','none')+line(272,92,W,92,C.line);
  o+=txt(298,64,title,12,C.ink,650)+icon('folder',298,69,13,C.faint)+txt(317,80,repo,8,C.faint,520,'start',mono);
  const color=status==='运行中'?C.orange:status==='已保存'||status==='空闲'?C.green:C.muted;
  o+=circle(1230,64,4,color)+txt(1242,68,status,9,color,650)+actions;
  return o;
}
function shell(content,{active='模型配置页重构',mode='work',title='模型配置页重构',repo='chatos-rs / main',status='运行中',actions=''}={}){return windowBar()+sidebar(active,mode)+topbar(title,repo,status,actions)+content}
function pageHead(x,y,title,sub,action=''){return txt(x,y,title,28,C.ink,650)+txt(x,y+27,sub,11,C.muted,450)+action}
function composer(x,y,w,placeholder='继续告诉 ChatOS 要做什么…'){
  let o=rect(x,y,w,116,C.paper,C.line,14,{filter:'url(#lift)'})+txt(x+18,y+28,placeholder,12,C.faint,450);
  o+=line(x+18,y+61,x+w-18,y+61,C.line2)+chip(x+18,y+75,'Local',C.green,C.greenSoft,64)+chip(x+90,y+75,'Workspace',C.purple,C.purpleSoft,88)+chip(x+186,y+75,'GPT-5',C.blue,C.blueSoft,66);
  o+=circle(x+w-31,y+88,18,C.ink)+icon('send',x+w-40,y+79,18,C.paper);
  return o;
}

function overview(){
  let b=windowBar()+logo(58,72,34,true)+chip(1380,76,'REVISION 03',C.orange,C.orangeSoft,116);
  b+=txt(58,157,'A focused operating space for AI work.',38,C.ink,620)+txt(58,189,'一个任务，一块主舞台；需要审查时，工具与产物才进入视野。',14,C.muted,450);
  b+=rect(58,232,1484,610,C.paper,C.line,16,{filter:'url(#shadow)'})+rect(58,232,260,610,C.side,'none',16)+line(318,232,318,842,C.line);
  b+=logo(78,254,28,true)+button(74,304,'新建任务','dark',228,'new');
  [['工作',true],['自动任务',false],['应用',false]].forEach(([n,a],i)=>{const y=362+i*37;if(a)b+=rect(70,y-20,236,32,C.paper,'none',8);b+=txt(100,y,n,10,a?C.ink:C.muted,a?650:500);});
  b+=txt(80,484,'最近会话',8,C.faint,700);
  sessions.slice(0,3).forEach(([n,s,,color],i)=>{const y=508+i*58;if(i===0)b+=rect(70,y-17,236,48,C.paper,'none',8);b+=circle(84,y,4,color)+txt(98,y+4,n,10,C.ink,i===0?650:500)+txt(98,y+20,s,7,C.faint,450);});
  b+=txt(344,270,'模型配置页重构',12,C.ink,650)+txt(344,290,'chatos-rs / main',8,C.faint,500,'start',mono)+button(1388,252,'提交','default',124);
  b+=line(318,310,1542,310,C.line)+rect(318,310,696,532,C.paper)+rect(1014,310,528,532,C.pane)+line(1014,310,1014,842,C.line);
  b+=txt(372,366,'你',9,C.muted,650)+rows(372,394,['重新设计模型配置页。先检查现有结构，','不要改变后端接口。'],12,C.text,21);
  b+=circle(348,465,14,C.ink)+icon('spark',340,457,16,C.paper)+txt(372,465,'ChatOS',9,C.orange,700)+rows(372,493,['我会先确认配置来源，再重组前端信息架构。','所有改动都会在右侧 Changes 中持续可审查。'],12,C.text,21);
  b+=rect(372,560,590,94,C.bg,C.line2,10)+txt(390,586,'已检查 7 个文件',10,C.ink,650)+txt(390,611,'读取设置页、模型配置和权限策略',9,C.muted,450)+txt(934,625,'18s',8,C.faint,500,'end',mono);
  b+=composer(350,688,624,'继续告诉 ChatOS 要做什么…');
  b+=txt(1038,348,'Changes',11,C.ink,650)+txt(1124,348,'Files',10,C.muted,500)+txt(1182,348,'Terminal',10,C.muted,500)+txt(1255,348,'Memory',10,C.muted,500)+line(1032,365,1524,365,C.line2);
  b+=txt(1038,400,'2 files changed',10,C.ink,650)+txt(1508,400,'+42  −18',9,C.green,650,'end',mono);
  [['AISettings.vue','+38','−16'],['ModelRoute.tsx','+4','−2']].forEach(([n,p,m],i)=>{const y=432+i*48;b+=icon('file',1038,y,15,C.muted)+txt(1062,y+12,n,9,C.text,600,'start',mono)+txt(1472,y+12,p,8,C.green,650,'end',mono)+txt(1510,y+12,m,8,C.orange,650,'end',mono);});
  const code=['- <ProviderCard />','+ <ModelMatrix>','+   <FallbackChain />','+ </ModelMatrix>'];code.forEach((s,i)=>b+=txt(1050,558+i*29,s,9,s.startsWith('+')?C.green:C.orange,500,'start',mono));
  b+=txt(58,902,'CHATOS / REVISION 03',9,C.faint,700)+txt(1538,902,'SESSION FIRST  ·  REVIEW ON DEMAND  ·  CONTEXT AT THE COMPOSER',9,C.faint,550,'end',mono);
  return doc('ChatOS UI Revision 03',b);
}

function login(){
  let b=windowBar()+rect(0,38,W,H-38,'#F6F6F3');
  b+=logo(52,66,34,true)+txt(100,244,'Your work,',52,C.ink,620)+txt(100,306,'kept in motion.',52,C.ink,620);
  b+=rows(102,358,['ChatOS 让任务、项目与执行结果保持在同一个连续工作空间。','不是更多控制台，而是更少的上下文切换。'],15,C.muted,28,430);
  [['01','并行推进多个长期任务'],['02','在会话旁直接审查文件与结果'],['03','从本机运行到长期记忆保持连续']].forEach(([n,t],i)=>{const y=500+i*75;b+=txt(102,y,n,9,i===0?C.orange:i===1?C.blue:C.green,700,'start',mono)+txt(146,y,t,13,C.text,580)+line(102,y+34,700,y+34,C.line2);});
  b+=rect(940,142,500,690,C.paper,C.line,18,{filter:'url(#shadow)'})+txt(990,212,'登录 ChatOS',27,C.ink,650)+txt(990,242,'继续进入你的工作空间',11,C.muted,450);
  b+=txt(990,304,'邮箱',9,C.muted,650)+rect(990,318,400,48,C.paper,C.line,9)+txt(1006,348,'name@company.com',11,C.faint,450);
  b+=txt(990,410,'密码',9,C.muted,650)+rect(990,424,400,48,C.paper,C.line,9)+txt(1006,455,'••••••••••••',13,C.ink,650)+txt(1368,454,'显示',9,C.green,650,'end');
  b+=button(990,508,'登录','dark',400)+line(990,588,1390,588,C.line2)+txt(990,620,'LOCAL WORKSPACE',8,C.faint,700);
  b+=circle(996,651,4,C.green)+txt(1010,655,'Local Connector 已连接',10,C.text,550)+circle(996,684,4,C.green)+txt(1010,688,'数据边界已验证',10,C.text,550);
  b+=txt(1190,784,'无需将本地密钥上传至 ChatOS',8,C.faint,450,'middle');
  return doc('ChatOS Login Revision 03',b);
}

function hub(){
  let c='';
  c+=rect(272,92,W-272,H-92,C.bg)+pageHead(330,162,'今天要推进什么？','从一个清晰的任务开始，ChatOS 会保留项目、执行和产物上下文。',button(1424,128,'新建任务','dark',122,'new'));
  c+=rect(330,220,1212,138,C.paper,C.line,14)+txt(356,249,'START A TASK',8,C.faint,700)+txt(356,289,'描述你想完成的结果…',17,C.faint,450)+line(356,313,1516,313,C.line2)+chip(356,322,'选择项目',C.muted,C.side,86)+chip(450,322,'Local',C.green,C.greenSoft,62)+txt(1498,342,'⌘ ↵',9,C.faint,600,'end',mono)+circle(1520,335,16,C.ink)+icon('send',1512,327,16,C.paper);
  c+=txt(330,418,'继续工作',14,C.ink,650)+txt(1518,418,'查看全部',9,C.muted,600,'end');
  const items=[['模型配置页重构','ChatOS','ChatOS 正在等待你确认设置域调整','12 分钟前',C.orange],['Task Runner 权限边界','Task Runner','184 个测试已通过','今天 10:42',C.green],['项目启动流程检查','Project Manager','启动与健康检查已完成','昨天',C.blue]];
  items.forEach(([n,p,d,t,color],i)=>{const y=442+i*90;c+=line(330,y+78,1518,y+78,C.line2)+circle(343,y+15,5,color)+txt(365,y+19,n,13,C.ink,620)+txt(365,y+44,d,10,C.muted,450)+chip(1264,y+2,p,C.muted,C.side,126)+txt(1518,y+22,t,9,C.faint,500,'end');});
  c+=txt(330,752,'项目',14,C.ink,650);
  [['ChatOS','Rust · React','main','18m'],['Task Runner','Rust','feature/runtime','1h'],['Project Manager','Rust · React','main','昨天']].forEach(([n,stack,branch,time],i)=>{const x=330+i*405;c+=rect(x,778,381,142,C.paper,C.line,12)+icon('folder',x+20,800,19,i===0?C.orange:C.muted)+txt(x+52,815,n,12,C.ink,650)+txt(x+20,848,stack,9,C.muted,500)+icon('branch',x+20,870,15,C.faint)+txt(x+43,882,branch,8,C.faint,520,'start',mono)+txt(x+355,882,time,8,C.faint,500,'end');});
  return doc('ChatOS Work Hub Revision 03',shell(c,{status:'空闲',actions:''}));
}

function chat(){
  let c=rect(272,92,786,908,C.paper)+rect(1058,92,542,908,C.pane)+line(1058,92,1058,1000,C.line);
  c+=txt(322,136,'会话',9,C.faint,700)+txt(322,170,'模型配置页重构',20,C.ink,650)+txt(322,194,'今天 10:39 · Code Agent',9,C.muted,500)+line(310,218,1028,218,C.line2);
  c+=txt(338,260,'你',9,C.muted,650)+rows(338,286,['设置页现在太长了。请重组模型、供应商与默认路由，','不要修改后端接口。'],13,C.text,23,450);
  c+=circle(322,366,15,C.ink)+icon('spark',314,358,16,C.paper)+txt(346,363,'ChatOS',9,C.orange,700)+rows(346,390,['我会先确认配置来源和权限边界，然后只调整前端信息结构。','右侧 Changes 会持续显示所有文件修改。'],13,C.text,23,450);
  c+=rect(346,458,640,98,C.bg,C.line2,10)+txt(364,484,'Explored 7 files',10,C.ink,650)+txt(364,509,'AISettings.vue、model-config.ts、permissions.ts…',9,C.muted,450)+txt(958,531,'18s',8,C.faint,500,'end',mono);
  c+=rows(346,604,['现有页面把供应商当成主结构，导致同一模型策略分散在多个表单。','我会把“模型路由”提升为主结构，供应商变成来源属性。'],13,C.text,23,450);
  c+=rect(346,686,640,84,C.orangeSoft,'none',10)+txt(364,712,'需要确认',9,C.orange,700)+txt(364,740,'将 Cloud AI 密钥入口移动到「连接」设置',11,C.ink,550)+button(862,710,'确认','dark',104);
  c+=composer(318,836,710);
  c+=txt(1084,127,'Changes',11,C.ink,650)+txt(1164,127,'Files',10,C.muted,500)+txt(1219,127,'Terminal',10,C.muted,500)+txt(1296,127,'Memory',10,C.muted,500)+line(1080,146,1574,146,C.line2)+line(1084,144,1140,144,C.ink,2);
  c+=txt(1084,184,'2 files changed',11,C.ink,650)+txt(1558,184,'+42  −18',9,C.green,650,'end',mono)+button(1412,204,'提交','default',146);
  [['AISettings.vue','+38','−16',true],['ModelRoute.tsx','+4','−2',false]].forEach(([n,p,m,a],i)=>{const y=268+i*50;if(a)c+=rect(1074,y-17,500,42,C.paper,'none',8);c+=icon('file',1086,y-3,16,a?C.blue:C.muted)+txt(1112,y+10,n,10,C.ink,600,'start',mono)+txt(1508,y+10,p,8,C.green,650,'end',mono)+txt(1558,y+10,m,8,C.orange,650,'end',mono);});
  c+=line(1084,372,1558,372,C.line2)+txt(1084,404,'AISettings.vue',9,C.faint,650,'start',mono);
  [['72','-','<ProviderList />','del'],['73','','',''],['74','+','<ModelRoute>','add'],['75','+','  <ModelMatrix />','add'],['76','+','  <FallbackChain />','add'],['77','+','</ModelRoute>','add']].forEach(([n,s,t,k],i)=>{const y=436+i*35;if(k)c+=rect(1074,y-21,500,31,k==='add'?C.greenSoft:C.orangeSoft);c+=txt(1100,y,n,8,C.faint,500,'end',mono)+txt(1122,y,s,9,k==='add'?C.green:C.orange,650,'middle',mono)+txt(1140,y,t,9,C.text,500,'start',mono);});
  c+=rect(1084,688,474,192,C.paper,C.line2,10)+txt(1102,718,'Memory',10,C.ink,650)+txt(1538,718,'24%',8,C.blue,650,'end')+txt(1102,750,'模型路由是主结构，供应商是来源属性。',10,C.text,550)+txt(1102,774,'来源：Task #184 · 产品决定',8,C.faint,450)+line(1102,798,1540,798,C.line2)+txt(1102,828,'本次会话上下文',8,C.faint,650)+rect(1102,844,436,7,C.side,'none',4)+rect(1102,844,105,7,C.blue,'none',4);
  return doc('ChatOS Session Revision 03',shell(c,{actions:button(1428,47,'打开项目','default',142)}));
}

function files(){
  let c=rect(272,92,236,908,C.side)+line(508,92,508,1000,C.line)+rect(508,92,638,908,C.paper)+rect(1146,92,454,908,C.pane)+line(1146,92,1146,1000,C.line);
  c+=txt(296,128,'Files',12,C.ink,650)+icon('search',464,113,17,C.muted);
  const f=[['chatos',0,'folder'],['frontend',1,'folder'],['src',2,'folder'],['views',3,'folder'],['AISettings.vue',3,'file'],['AgentChat.vue',3,'file'],['components',2,'folder'],['backend',1,'folder'],['README.md',1,'file']];
  f.forEach(([n,d,ico],i)=>{const y=170+i*33;if(n==='AISettings.vue')c+=rect(284,y-20,212,29,C.blueSoft,'none',6);c+=icon(ico,296+d*16,y-15,15,n==='AISettings.vue'?C.blue:C.muted)+txt(317+d*16,y,n,9,n==='AISettings.vue'?C.blue:C.text,n==='AISettings.vue'?650:500,'start',ico==='file'?mono:sans);});
  c+=txt(532,128,'AISettings.vue',10,C.ink,650,'start',mono)+chip(1018,108,'已修改',C.orange,C.orangeSoft,78)+line(508,148,1146,148,C.line);
  c+=txt(532,174,'Diff',9,C.ink,700)+txt(580,174,'File',9,C.muted,500)+txt(1120,174,'−18  +42',8,C.muted,600,'end',mono)+line(508,190,1146,190,C.line);
  const code=[['72','-','<section class="providers">','del'],['73','-','  <ProviderCard />','del'],['74','-','</section>','del'],['75','','',''],['76','+','<ModelRoute>','add'],['77','+','  <ModelMatrix :models="models" />','add'],['78','+','  <FallbackChain />','add'],['79','+','</ModelRoute>','add'],['80','','',''],['81','+','<PolicySource scope="workspace" />','add']];
  code.forEach(([n,s,t,k],i)=>{const y=226+i*39;if(k)c+=rect(508,y-24,638,34,k==='add'?C.greenSoft:C.orangeSoft);c+=txt(542,y,n,8,C.faint,500,'end',mono)+txt(565,y,s,9,k==='add'?C.green:C.orange,650,'middle',mono)+txt(586,y,t,9,C.text,500,'start',mono);});
  c+=rect(538,674,578,170,C.bg,C.line2,10)+txt(558,704,'Change summary',10,C.ink,650)+rows(558,734,['供应商从页面主结构降为模型来源属性。','默认模型、回退链和策略来源在同一视野内完成审查。'],10,C.muted,21,450)+chip(558,794,'UI ONLY',C.blue,C.blueSoft,74)+chip(640,794,'NO API CHANGE',C.green,C.greenSoft,112);
  c+=txt(1172,128,'Preview',11,C.ink,650)+txt(1572,128,'Settings / AI',8,C.faint,500,'end')+line(1168,148,1576,148,C.line2);
  c+=txt(1174,192,'模型路由',19,C.ink,650)+txt(1174,217,'工作区默认策略',9,C.muted,450);
  [['GPT-5','默认',C.blue,true],['Claude Sonnet','回退 1',C.purple,false],['Local Qwen','回退 2',C.green,false]].forEach(([n,s,color,on],i)=>{const y=252+i*78;c+=rect(1174,y,398,62,C.paper,C.line2,9)+circle(1196,y+22,5,color)+txt(1210,y+25,n,10,C.ink,650)+txt(1210,y+46,s,8,C.faint,500)+circle(1546,y+31,8,on?color:C.paper,color,1.5);});
  c+=line(1174,500,1572,500,C.line2);[['来源','Workspace',C.purple],['失败策略','自动回退',C.green],['权限','按任务决定',C.orange]].forEach(([a,b,color],i)=>{const y=536+i*37;c+=txt(1174,y,a,9,C.faint,650)+txt(1572,y,b,9,color,650,'end');});
  c+=button(1174,668,'应用更改','dark',398)+txt(1174,754,'Review',10,C.ink,650);[['类型检查通过',C.green],['组件测试通过',C.green],['等待视觉确认',C.orange]].forEach(([n,color],i)=>{const y=784+i*35;c+=circle(1178,y-3,4,color)+txt(1192,y,n,9,C.text,520);});
  return doc('ChatOS Files Revision 03',shell(c,{status:'已保存',actions:button(1428,47,'提交','default',142)}));
}

function plan(){
  const graphRight=1210;
  let c=rect(272,92,graphRight-272,908,C.bg)+rect(272,92,graphRight-272,908,'url(#graphDots)')+rect(graphRight,92,W-graphRight,908,C.pane)+line(graphRight,92,graphRight,1000,C.line);
  const edge=(d,color=C.muted,sw=1.7,dash)=>pathEl(d,color,sw,'none',{markerEnd:'url(#graphArrow)',strokeDasharray:dash});
  c+=edge('M730 328V364H520V405',C.faint)+edge('M730 328V364H890V405',C.faint);
  c+=edge('M520 523V558H725V600',C.green,2)+edge('M890 523V558H725V600',C.green,2);
  c+=edge('M725 740V772H520V810',C.orange,2)+edge('M725 740V772H890V810',C.orange,2);
  c+=pathEl('M860 269C1080 300 1085 520 875 653',C.purple,1.4,'none',{markerEnd:'url(#graphArrowContext)',strokeDasharray:'7 7',opacity:.7});
  c+=rect(294,112,390,38,C.paper,C.line,19,{filter:'url(#lift)'})+circle(316,131,5,C.orange)+txt(329,135,'当前消息',9,C.text,600)+circle(402,131,5,C.yellow)+txt(415,135,'直接前置',9,C.text,600)+circle(496,131,5,C.faint)+txt(509,135,'间接前置',9,C.text,600)+line(590,131,610,131,C.purple,1.5,{strokeDasharray:'5 4'})+txt(620,135,'上下文',9,C.text,600);
  c+=rect(920,112,266,38,C.paper,C.line,8,{filter:'url(#lift)'})+rect(925,117,72,28,C.ink,'none',6)+txt(961,135,'精简图',9,C.paper,650,'middle')+txt(1036,135,'完整图',9,C.muted,550,'middle')+line(1080,120,1080,142,C.line)+txt(1101,135,'清除聚焦',9,C.muted,550);
  const node=(x,y,w,h,{relation,status,title,desc,tone='neutral',meta='前置依赖 0',run='暂无运行记录',active=false})=>{
    const stroke=active?C.orange:tone==='direct'?'#DDB96F':C.line,fill=active?'#FFFCFB':tone==='direct'?'#FFFDF7':C.paper;
    let o=rect(x,y,w,h,fill,stroke,12,{strokeWidth:active?2:1,filter:'url(#lift)'});
    if(active)o+=rect(x,y,4,h,C.orange,'none',3);
    const rc=active?C.orange:tone==='direct'?C.yellow:C.muted,rf=active?C.orangeSoft:tone==='direct'?C.yellowSoft:C.side;
    o+=chip(x+14,y+13,relation,rc,rf,relation==='当前消息'?72:relation==='直接前置'?78:76);
    o+=chip(x+100,y+13,status,status==='执行中'?C.orange:status==='已完成'?C.green:status==='阻塞'?C.orange:C.muted,status==='执行中'?C.orangeSoft:status==='已完成'?C.greenSoft:status==='阻塞'?C.orangeSoft:C.side,72);
    o+=txt(x+14,y+58,title,11,C.ink,650)+txt(x+14,y+80,desc,8,C.muted,450);
    o+=line(x+14,y+h-30,x+w-14,y+h-30,C.line2)+txt(x+14,y+h-11,meta,8,C.faint,500)+txt(x+w-14,y+h-11,run,8,C.faint,500,'end');
    return o;
  };
  c+=node(600,210,260,118,{relation:'间接前置',status:'已完成',title:'读取现有模型配置',desc:'识别模型、供应商与策略来源',run:'有运行记录'});
  c+=node(390,405,260,118,{relation:'直接前置',status:'已完成',title:'分析配置继承关系',desc:'Workspace / User / Environment',tone:'direct',run:'有运行记录'});
  c+=node(760,405,260,118,{relation:'直接前置',status:'已完成',title:'检查权限与密钥入口',desc:'确认敏感配置的数据边界',tone:'direct',run:'有运行记录'});
  c+=node(575,600,300,140,{relation:'当前消息',status:'执行中',title:'重构模型路由界面',desc:'建立模型矩阵、默认路由与回退链',meta:'前置依赖 2',run:'Run #184',active:true});
  c+=node(390,810,260,118,{relation:'后续任务',status:'等待中',title:'补充键盘导航',desc:'完成模型矩阵的无鼠标操作',meta:'前置依赖 1'});
  c+=node(760,810,260,118,{relation:'后续任务',status:'等待中',title:'视觉回归验证',desc:'检查宽屏与紧凑窗口布局',meta:'前置依赖 1'});
  c+=rect(1020,946,166,38,C.paper,C.line,8,{filter:'url(#lift)'})+txt(1044,970,'−',15,C.muted,500,'middle')+txt(1093,970,'86%',9,C.text,650,'middle',mono)+txt(1142,970,'＋',14,C.muted,500,'middle')+line(1067,954,1067,977,C.line)+line(1119,954,1119,977,C.line);
  c+=txt(1236,130,'Task inspector',11,C.ink,650)+txt(1570,130,'Run #184',8,C.faint,550,'end',mono)+line(1232,150,1572,150,C.line2);
  c+=chip(1234,176,'当前消息',C.orange,C.orangeSoft,76)+chip(1320,176,'执行中',C.orange,C.orangeSoft,70);
  c+=txt(1234,236,'重构模型路由界面',16,C.ink,650)+rows(1234,266,['建立模型矩阵、默认路由与回退链，','只调整前端信息结构。'],10,C.muted,20,450);
  c+=txt(1234,330,'依赖',8,C.faint,700)+txt(1570,330,'2 个直接前置',9,C.text,600,'end')+line(1234,350,1570,350,C.line2);
  c+=txt(1234,382,'执行信息',8,C.faint,700);[['模型','GPT-5',C.blue],['权限','Workspace',C.purple],['分支','task/184-model-route',C.text],['已运行','12m 18s',C.text]].forEach(([a,b,color],i)=>{const y=416+i*36;c+=txt(1234,y,a,8,C.faint,550)+txt(1570,y,b,8,color,600,'end',a==='分支'?mono:sans);});
  c+=line(1234,574,1570,574,C.line2)+txt(1234,606,'最近进展',8,C.faint,700)+circle(1238,636,4,C.green)+txt(1252,640,'已完成配置来源检查',9,C.text,550)+circle(1238,672,4,C.orange)+txt(1252,676,'正在修改 AISettings.vue',9,C.text,550)+circle(1238,708,4,C.faint)+txt(1252,712,'等待视觉验证',9,C.muted,500);
  c+=button(1234,770,'执行过程','default',158,'plan')+button(1412,770,'任务详情','default',158,'file')+button(1234,820,'查看 Run','dark',336);
  c+=rect(1234,884,336,72,C.orangeSoft,'none',9)+txt(1250,910,'聚焦模式',8,C.orange,700)+txt(1250,934,'上游 3 个 · 下游 2 个 · 其他节点已降噪',9,C.text,520);
  return doc('ChatOS Task Flow Revision 03',shell(c,{title:'Task flow',repo:'当前消息 / Task Runner',status:'运行中',actions:button(1430,47,'完整图','default',138)}));
}

function runtime(){
  let c=rect(272,92,914,908,C.terminal)+rect(1186,92,414,908,C.pane)+line(1186,92,1186,1000,C.line);
  c+=rect(272,92,914,46,C.terminal2)+circle(294,115,5,'#CA8175')+circle(312,115,5,'#CAA25D')+circle(330,115,5,'#6FA189')+txt(356,119,'ChatOS · zsh',9,'#AEB4AA',550,'start',mono)+chip(1042,102,'workspace',C.terminalText,'#373B34',108);
  const t=[['$','cargo test -p task_runner_service',C.terminalText],['','running 184 tests…','#AEB4AA'],['','test result: ok. 184 passed; 0 failed','#83BE96'],['','',''],['$','pnpm --dir chatos/frontend test',C.terminalText],['','✓ 42 tests passed in 3.8s','#83BE96'],['','',''],['$','pnpm --dir chatos/frontend dev',C.terminalText],['','Local: http://localhost:8088/','#D2AB6E'],['','ready in 640 ms','#83BE96'],['','',''],['$','_',C.terminalText]];
  t.forEach(([p,s,color],i)=>{const y=184+i*43;c+=txt(304,y,p,11,p?C.orange:color,650,'start',mono)+txt(330,y,s,11,color||'#AEB4AA',500,'start',mono);});
  c+=txt(1212,130,'Services',11,C.ink,650)+line(1210,150,1574,150,C.line2);
  [['Frontend','localhost:8088',C.green],['ChatOS API',':3200',C.green],['Task Runner',':3210',C.orange],['Project Manager',':3220',C.green]].forEach(([n,a,color],i)=>{const y=190+i*62;c+=circle(1216,y-3,4,color)+txt(1230,y,n,10,C.ink,600)+txt(1572,y,a,8,C.faint,500,'end',mono);});
  c+=txt(1212,456,'Execution context',11,C.ink,650)+line(1210,476,1574,476,C.line2);
  [['Directory','/project/chatos_rs'],['Branch','codex/ui-r3'],['Permission','Workspace'],['Model','GPT-5'],['Environment','Local']].forEach(([a,b],i)=>{const y=516+i*42;c+=txt(1212,y,a,8,C.faint,650)+txt(1572,y,b,8,a==='Permission'?C.purple:C.text,600,'end',mono);});
  c+=txt(1212,756,'Recent runs',11,C.ink,650)+line(1210,776,1574,776,C.line2);[['UI generator','18s',C.green],['SVG validation','1.2s',C.green],['Visual QA','waiting',C.orange]].forEach(([a,b,color],i)=>{const y=816+i*42;c+=circle(1216,y-3,4,color)+txt(1230,y,a,9,C.text,550)+txt(1572,y,b,8,C.faint,500,'end',mono);});
  return doc('ChatOS Runtime Revision 03',shell(c,{title:'Runtime',repo:'chatos-rs / codex-ui-r3',actions:button(1428,47,'启动全部','accent',142)}));
}

function settings(){
  let c=rect(272,92,218,908,C.side)+rect(490,92,1110,908,C.bg)+line(490,92,490,1000,C.line);
  c+=txt(300,132,'Settings',16,C.ink,650);[['个人',false],['连接',false],['AI 与模型',true],['权限',false],['记忆',false],['快捷键',false]].forEach(([n,a],i)=>{const y=174+i*45;if(a)c+=rect(288,y-23,190,35,C.blueSoft,'none',8);c+=txt(306,y,n,10,a?C.blue:C.muted,a?650:500);});
  c+=pageHead(536,154,'AI 与模型','管理模型可用性与任务路由。密钥归连接设置管理。',button(1426,126,'保存更改','dark',126));
  c+=txt(536,222,'Default route',10,C.faint,700)+rect(536,240,1018,72,C.paper,C.line,11)+chip(558,263,'GPT-5',C.blue,C.blueSoft,72)+txt(642,280,'→',11,C.faint,650)+chip(674,263,'Claude Sonnet',C.purple,C.purpleSoft,118)+txt(804,280,'→',11,C.faint,650)+chip(836,263,'Local Qwen',C.green,C.greenSoft,102)+txt(1528,283,'编辑路由',9,C.blue,650,'end');
  c+=txt(536,366,'Available models',12,C.ink,650)+txt(1554,366,'3 providers · 5 models',8,C.faint,550,'end');
  c+=rect(536,388,1018,330,C.paper,C.line,11);
  const xs=[558,830,1030,1165,1320,1518];['模型','来源','上下文','工具','角色','启用'].forEach((n,i)=>c+=txt(xs[i],420,n,8,C.faint,700,i===5?'end':'start'));
  const m=[['GPT-5','Cloud AI','128k','完整','默认',C.blue,true],['Claude Sonnet','Anthropic','200k','完整','回退 1',C.purple,true],['Local Qwen','Local','32k','受限','回退 2',C.green,true],['Gemini Pro','Cloud AI','1m','完整','—',C.blue,false],['DeepSeek R1','Local','64k','受限','—',C.green,false]];
  m.forEach(([n,src,ctx,tool,role,color,on],i)=>{const y=466+i*50;if(i%2)c+=rect(537,y-24,1016,48,C.pane);c+=circle(560,y-3,4,color)+txt(574,y,n,9,C.ink,650)+txt(830,y,src,8,C.muted,500)+txt(1030,y,ctx,8,C.text,520,'start',mono)+txt(1165,y,tool,8,C.muted,500)+txt(1320,y,role,8,role==='—'?C.faint:color,650)+rect(1480,y-15,38,21,on?color:C.side,'none',11)+circle(on?1507:1491,y-5,8,C.paper);});
  c+=rect(536,746,498,180,C.paper,C.line,11)+txt(558,778,'Configuration source',11,C.ink,650);[['Workspace','优先',C.purple],['User preferences','继承',C.muted],['Environment','3 variables',C.muted]].forEach(([a,b,color],i)=>{const y=818+i*32;c+=txt(558,y,a,8,color,600)+txt(1012,y,b,8,C.faint,500,'end');});
  c+=rect(1056,746,498,180,C.paper,C.line,11)+txt(1078,778,'Routing policy',11,C.ink,650)+rows(1078,818,['不可用时按回退链顺序切换。','Local 模型不向外部发送项目内容。','工具权限由当前任务决定。'],9,C.muted,26,450);
  return doc('ChatOS Settings Revision 03',shell(c,{mode:'work',title:'Settings',repo:'Workspace preferences',status:'未保存'}));
}

function agents(){
  let c=rect(272,92,W-272,908,C.bg)+pageHead(326,158,'Agents & apps','定义 ChatOS 如何工作，以及任务可以使用哪些连接。',button(1414,126,'新建 Agent','dark',134,'new'));
  c+=txt(326,228,'Agents',11,C.ink,650)+txt(930,228,'Apps & connections',11,C.ink,650);
  const ag=[['Code Agent','实现、测试与代码审查','GPT-5',C.green,true],['Research Agent','资料检索与综合分析','Claude',C.blue,false],['Product Agent','需求、流程与界面设计','GPT-5',C.purple,false],['Ops Agent','服务运行与环境诊断','Local',C.orange,false]];
  ag.forEach(([n,d,m,color,a],i)=>{const y=254+i*130;c+=rect(326,y,562,112,a?C.paper:C.bg,a?C.line:C.line2,11)+circle(356,y+34,17,color)+txt(356,y+38,n[0],9,C.paper,700,'middle')+txt(386,y+32,n,11,C.ink,650)+txt(386,y+57,d,9,C.muted,450)+chip(386,y+72,m,color,color===C.green?C.greenSoft:color===C.blue?C.blueSoft:color===C.purple?C.purpleSoft:C.orangeSoft,82)+(a?chip(790,y+16,'当前',C.green,C.greenSoft,68):'');});
  const ap=[['Local Files','工作区文件','已连接',C.green,'folder'],['Terminal','命令与服务','已连接',C.green,'terminal'],['Browser','网页与预览','可用',C.blue,'apps'],['Project Manager','项目与需求','已连接',C.green,'plan'],['Cloud AI','外部模型','需配置',C.orange,'spark']];
  ap.forEach(([n,d,s,color,ico],i)=>{const y=254+i*104;c+=rect(930,y,624,88,C.paper,C.line2,10)+rect(948,y+18,44,44,C.bg,C.line2,9)+icon(ico,960,y+30,20,color)+txt(1010,y+31,n,10,C.ink,650)+txt(1010,y+53,d,8,C.muted,450)+circle(1464,y+42,4,color)+txt(1478,y+46,s,8,color,650);});
  return doc('ChatOS Agents Apps Revision 03',shell(c,{active:'Agent 角色系统',mode:'apps',title:'Agents & apps',repo:'Workspace capabilities',status:'空闲'}));
}

function notes(){
  let c=rect(272,92,232,908,C.side)+rect(504,92,1096,908,C.paper)+line(504,92,504,1000,C.line);
  c+=txt(296,130,'Notes',13,C.ink,650)+button(402,109,'新建','default',82,'new');
  [['模型设置改版','今天 11:02',true],['运行时排查记录','今天 09:18',false],['产品定位草稿','昨天',false],['快捷键清单','8 月 19 日',false]].forEach(([n,t,a],i)=>{const y=166+i*70;if(a)c+=rect(284,y-22,208,57,C.yellowSoft,'none',8);c+=txt(300,y,n,10,a?C.ink:C.muted,a?650:500)+txt(300,y+21,t,8,C.faint,450);});
  c+=txt(558,160,'模型设置改版',28,C.ink,650)+txt(558,188,'最后编辑于今天 11:02 · ChatOS',9,C.faint,450)+line(558,214,1548,214,C.line2);
  c+=txt(558,272,'目标',16,C.ink,650)+rows(558,304,['把模型、供应商、默认路由和配置来源放到同一工作面，','避免在多个长表单之间来回跳转。'],12,C.text,23,450);
  c+=rect(558,378,990,76,C.yellowSoft,'none',9)+txt(578,403,'KEY CONSTRAINT',8,C.yellow,700)+txt(578,430,'科技感来自可见的执行过程与精确的信息结构，不来自深色霓虹。',11,C.ink,550);
  c+=txt(558,516,'页面结构',16,C.ink,650);
  [['01','模型路由','默认、回退与启用状态'],['02','配置来源','Workspace、User、Environment'],['03','策略说明','失败处理、数据边界与权限']].forEach(([n,t,d],i)=>{const y=564+i*74;c+=txt(558,y,n,8,C.faint,650,'start',mono)+txt(598,y,t,11,C.ink,650)+txt(598,y+23,d,9,C.muted,450);});
  c+=line(558,804,1548,804,C.line2)+txt(558,842,'Linked context',8,C.faint,700)+chip(558,860,'ChatOS',C.green,C.greenSoft,72)+chip(638,860,'AI Settings',C.blue,C.blueSoft,92)+chip(738,860,'Task #184',C.purple,C.purpleSoft,88)+txt(1548,924,'286 words · Markdown',8,C.faint,500,'end');
  return doc('ChatOS Notes Revision 03',shell(c,{title:'Notes',repo:'ChatOS / model settings',status:'已保存'}));
}

function contextEditor(){
  let c=rect(272,92,234,908,C.side)+rect(506,92,1094,908,C.bg)+line(506,92,506,1000,C.line);
  c+=txt(296,130,'System context',13,C.ink,650)+txt(296,151,'Versioned agent instructions',8,C.faint,450);
  [['Code Agent','v12',true],['Research Agent','v5',false],['Product Agent','v8',false],['Ops Agent','v3',false]].forEach(([n,v,a],i)=>{const y=192+i*54;if(a)c+=rect(284,y-23,210,43,C.purpleSoft,'none',8);c+=circle(300,y-2,4,a?C.purple:C.line)+txt(314,y+1,n,9,a?C.ink:C.muted,a?650:500)+txt(480,y+1,v,7,C.faint,500,'end',mono);});
  c+=pageHead(548,154,'Code Agent / System Context','基础规则、角色能力、项目覆盖与当前任务按顺序合成。',button(1422,126,'发布 v13','dark',126));
  c+=rect(548,214,1004,54,C.paper,C.line,9)+chip(566,228,'DRAFT v13',C.orange,C.orangeSoft,98)+txt(680,246,'基于 v12 · 6 处修改',8,C.muted,500)+txt(1528,246,'查看历史',8,C.purple,650,'end');
  c+=rect(548,294,712,624,C.paper,C.line,10)+rect(548,294,712,42,C.pane,'none',10)+txt(566,320,'system-context.md',8,C.ink,650,'start',mono)+txt(1238,320,'Markdown',7,C.faint,500,'end');
  const l=[['01','# Role',C.purple],['02','You are the Code Agent for ChatOS.',C.text],['03','Implement scoped changes and verify them.',C.text],['04','',C.text],['05','## Working principles',C.purple],['06','- Keep task context visible before acting.',C.text],['07','- Present changes as reviewable runs.',C.text],['08','- Ask before expanding permissions.',C.orange],['09','- Record durable conclusions in Memory.',C.blue],['10','',C.text],['11','## Tool policy',C.purple],['12','Use workspace tools within the active project.',C.text],['13','Never expose secrets in logs or artifacts.',C.orange],['14','',C.text],['15','## Completion',C.purple],['16','Report files, checks, and remaining risks.',C.text]];
  l.forEach(([n,s,color],i)=>{const y=370+i*31;c+=txt(576,y,n,7,C.faint,500,'end',mono)+txt(594,y,s,9,color,500,'start',mono);});
  c+=txt(1290,322,'Composition',10,C.ink,650);[['Base','全局规则',C.purple],['Agent','角色能力',C.blue],['Project','项目约束',C.green],['Task','当前指令',C.orange]].forEach(([a,b,color],i)=>{const y=356+i*52;c+=rect(1288,y,264,40,C.paper,C.line2,8)+circle(1304,y+20,4,color)+txt(1318,y+23,a,9,C.ink,650)+txt(1534,y+23,b,7,C.faint,500,'end');});
  c+=txt(1290,608,'Publish checks',10,C.ink,650);[['变量语法','通过',C.green],['冲突规则','通过',C.green],['权限扩张','无',C.green],['Token 估算','1.8k',C.blue],['测试会话','待运行',C.orange]].forEach(([a,b,color],i)=>{const y=650+i*43;c+=txt(1290,y,a,8,C.muted,550)+txt(1534,y,b,8,color,650,'end');});
  return doc('ChatOS System Context Revision 03',shell(c,{active:'Agent 角色系统',title:'System context',repo:'Code Agent / draft v13',status:'草稿'}));
}

function designSystem(){
  let b=windowBar()+logo(54,68,34,true)+txt(54,142,'Desktop design system',32,C.ink,650)+txt(54,174,'Revision 03 · production rules for every ChatOS workspace surface',12,C.muted,450);
  b+=txt(54,226,'FOUNDATION',9,C.faint,700);
  const colors=[['Ink',C.ink],['Canvas',C.bg],['Surface',C.paper],['Execution',C.orange],['Success',C.green],['Model',C.blue],['Permission',C.purple],['Attention',C.yellow]];
  colors.forEach(([n,color],i)=>{const x=54+i*185;b+=rect(x,246,164,78,C.paper,C.line,9)+rect(x+12,258,42,42,color,color===C.paper?C.line:'none',8)+txt(x+66,277,n,10,C.ink,650)+txt(x+66,298,color,8,C.faint,500,'start',mono);});
  b+=txt(54,370,'TYPE & SPACING',9,C.faint,700)+rect(54,392,720,188,C.paper,C.line,11)+txt(78,433,'Display / 32',32,C.ink,650)+txt(78,472,'Page title / 28',28,C.ink,650)+txt(78,510,'Section title / 14',14,C.ink,650)+txt(420,431,'正文负责解释，界面负责组织。',14,C.text,450)+txt(420,465,'Metadata / 9px / Muted',9,C.muted,500)+txt(420,499,'task/184-model-route',9,C.blue,550,'start',mono)+rows(420,536,['Base unit 4px · content gaps 16/24/32','Radius 8/10/12/14 · border 1px'],9,C.faint,20,500);
  b+=rect(800,392,746,188,C.paper,C.line,11)+txt(824,424,'CORE CONTROLS',9,C.faint,700)+button(824,449,'主要操作','dark',112)+button(948,449,'次要操作','default',112)+button(1072,449,'执行操作','accent',112)+chip(824,510,'执行中',C.orange,C.orangeSoft,72)+chip(904,510,'已完成',C.green,C.greenSoft,72)+chip(984,510,'Workspace',C.purple,C.purpleSoft,88)+chip(1080,510,'GPT-5',C.blue,C.blueSoft,64)+rect(1190,449,304,78,C.paper,C.line,12)+txt(1210,477,'Composer control group',10,C.ink,650)+chip(1210,491,'Local',C.green,C.greenSoft,60)+chip(1278,491,'Workspace',C.purple,C.purpleSoft,82)+chip(1368,491,'GPT-5',C.blue,C.blueSoft,62);
  b+=txt(54,630,'LAYOUT CONTRACTS',9,C.faint,700);
  [['Session','272 sidebar + stage + optional inspector'],['Project','sidebar + local tree + content + preview'],['Task graph','DAG canvas + task inspector'],['Modal','focused workbench, not a small form']].forEach(([n,d],i)=>{const x=54+i*373;b+=rect(x,652,350,154,C.paper,C.line,11)+txt(x+18,681,n,12,C.ink,650)+txt(x+18,705,d,9,C.muted,450)+rect(x+18,730,314,54,C.bg,C.line2,7);if(i===0){b+=rect(x+18,730,70,54,C.side)+line(x+88,730,x+88,784,C.line)+rect(x+230,730,102,54,C.pane)+line(x+230,730,x+230,784,C.line);}if(i===1){b+=rect(x+18,730,62,54,C.side)+rect(x+80,730,145,54,C.paper)+rect(x+225,730,107,54,C.pane)+line(x+80,730,x+80,784,C.line)+line(x+225,730,x+225,784,C.line);}if(i===2){b+=rect(x+18,730,220,54,'url(#graphDots)')+rect(x+238,730,94,54,C.pane)+line(x+238,730,x+238,784,C.line);}if(i===3){b+=rect(x+42,738,266,38,C.paper,C.line,8,{filter:'url(#lift)'});}});
  b+=txt(54,866,'NON-NEGOTIABLE',9,C.faint,700)+rows(54,894,['One dominant task surface · Inspector appears only for the active artifact · No dashboard filler','Readable text at native size · Status never relies on color alone · Terminal dark mode stays local'],10,C.text,24,520)+txt(1546,952,'CHATOS DESIGN SYSTEM / R03',8,C.faint,600,'end',mono);
  return doc('ChatOS Design System Revision 03',b);
}

function taskBlocked(){
  let c=rect(272,92,1328,908,C.bg,{opacity:.55});
  c+=rect(456,126,960,800,C.paper,C.line,16,{filter:'url(#shadow)'})+txt(490,170,'任务节点处理',17,C.ink,650)+txt(1380,170,'×',16,C.muted,500,'end')+line(476,192,1396,192,C.line2);
  c+=chip(490,220,'BLOCKED',C.orange,C.orangeSoft,82)+chip(582,220,'当前消息',C.orange,C.orangeSoft,78)+txt(490,280,'配置密钥迁移',23,C.ink,650)+txt(490,309,'节点进入阻塞状态，后续依赖任务不会继续调度。',10,C.muted,450);
  c+=rect(490,344,892,112,C.orangeSoft,'none',10)+txt(510,373,'阻塞原因',9,C.orange,700)+rows(510,401,['Cloud AI 密钥仍由旧设置页直接管理，无法确认迁移后是否影响用户级配置。','需要补充处理意见，或将该可选节点标记为成功后继续。'],10,C.text,21,450);
  c+=txt(490,500,'Dependency impact',10,C.faint,700);[['上游','检查配置来源','已完成',C.green],['当前','配置密钥迁移','阻塞',C.orange],['下游','视觉与键盘验证','等待',C.faint]].forEach(([a,b,s,color],i)=>{const y=536+i*55;c+=txt(490,y,a,8,C.faint,650)+circle(548,y-3,4,color)+txt(562,y,b,10,C.text,600)+txt(1368,y,s,9,color,650,'end');if(i<2)c+=line(562,y+25,1368,y+25,C.line2);});
  c+=txt(490,720,'处理意见',9,C.faint,700)+rect(490,738,892,82,C.paper,C.line,9)+txt(510,768,'补充如何迁移密钥配置，ChatOS 会仅重新运行当前节点…',10,C.faint,450);
  c+=line(476,846,1396,846,C.line2)+button(490,868,'取消','default',100)+button(982,868,'跳过可选节点','default',176)+button(1174,868,'重新处理此节点','dark',208);
  return doc('ChatOS Blocked Task Revision 03',shell(c,{title:'Task flow',repo:'Task #184 / blocked',status:'阻塞'}));
}

function taskRunDetail(){
  let c=rect(272,92,1328,908,C.bg)+rect(330,118,1212,830,C.paper,C.line,14,{filter:'url(#shadow)'});
  c+=txt(362,158,'Run #184',17,C.ink,650)+chip(462,140,'SUCCEEDED',C.green,C.greenSoft,92)+txt(1508,158,'×',16,C.muted,500,'end')+line(350,182,1522,182,C.line2);
  c+=txt(362,222,'重构模型路由界面',20,C.ink,650)+txt(362,248,'GPT-5 · Workspace · task/184-model-route · 12m 18s',9,C.muted,500,'start',mono);
  c+=rect(362,282,770,620,C.pane,C.line2,10)+txt(386,314,'Execution timeline',11,C.ink,650);
  const ev=[['10:39:02','Run created','输入与权限快照已保存',C.green],['10:39:11','Model phase','生成前端修改计划',C.blue],['10:41:08','Workspace','创建隔离分支并应用修改',C.purple],['10:46:32','Verification','类型检查与 42 个测试通过',C.green],['10:51:20','Integration','结果提交已生成',C.green]];
  ev.forEach(([t,n,d,color],i)=>{const y=360+i*92;c+=circle(390,y,8,C.paper,color,2)+(i<4?line(390,y+9,390,y+82,C.line,2):'');c+=txt(420,y-2,n,11,C.ink,650)+txt(420,y+22,d,9,C.muted,450)+txt(1102,y+1,t,8,C.faint,500,'end',mono);});
  c+=txt(1164,314,'Run summary',11,C.ink,650);[['Model','GPT-5'],['Tokens','28.4k'],['Files','2 changed'],['Commit','9f21c7a'],['Memory','3 conclusions']].forEach(([a,b],i)=>{const y=356+i*42;c+=txt(1164,y,a,8,C.faint,650)+txt(1508,y,b,8,a==='Commit'?C.blue:C.text,600,'end',a==='Commit'?mono:sans);});
  c+=line(1164,564,1508,564,C.line2)+txt(1164,598,'Changed files',10,C.ink,650);[['AISettings.vue','+38 −16'],['ModelRoute.tsx','+4 −2']].forEach(([n,d],i)=>{const y=634+i*38;c+=icon('file',1164,y-14,15,C.muted)+txt(1188,y,n,8,C.text,600,'start',mono)+txt(1508,y,d,8,C.green,600,'end',mono);});
  c+=rect(1164,740,344,108,C.greenSoft,'none',9)+txt(1182,768,'Result',9,C.green,700)+rows(1182,794,['模型路由结构已完成，','所有验证通过。'],9,C.text,20,500)+button(1164,866,'打开提交','dark',344);
  return doc('ChatOS Task Run Detail Revision 03',shell(c,{title:'Run detail',repo:'Task #184 / Run #184',status:'已完成'}));
}

function projectPlanWorkspace(){
  let c=rect(272,92,1328,908,C.bg)+rect(272,92,1328,48,C.paper)+txt(300,122,'Files',9,C.muted,500)+txt(354,122,'Team',9,C.muted,500)+txt(408,122,'Plan',9,C.ink,700)+txt(456,122,'Settings',9,C.muted,500)+line(398,138,440,138,C.orange,2)+line(272,140,1600,140,C.line);
  c+=rect(272,140,670,860,C.side)+rect(942,140,658,860,C.paper)+line(942,140,942,1000,C.line);
  c+=rect(292,160,630,52,C.paper,C.line2,8);[['Requirements','12'],['Done','7'],['Blocked','1']].forEach(([a,b],i)=>{const x=312+i*190;c+=txt(x,183,a,8,C.faint,650)+txt(x,202,b,14,C.ink,650);});
  const cols=[['Product goals',[['模型设置重构','执行中'],['会话运行上下文','完成'],['远程资源管理','计划']]],['Features',[['模型路由矩阵','执行中'],['密钥连接设置','阻塞'],['键盘导航','计划']]],['Implementation',[['AISettings.vue','进行中'],['Route policy','完成'],['Visual QA','等待']]]];
  cols.forEach(([title,items],ci)=>{const x=292+ci*206;c+=txt(x,246,title,10,C.ink,650)+chip(x+148,230,String(items.length),C.muted,C.paper,38);items.forEach(([n,s],i)=>{const y=266+i*134;const active=n==='模型路由矩阵';c+=rect(x,y,188,116,active?C.paper:C.bg,active?C.orange:C.line2,9)+txt(x+12,y+24,n,10,C.ink,650)+chip(x+12,y+37,s,s==='完成'?C.green:s==='阻塞'?C.orange:s==='执行中'||s==='进行中'?C.orange:C.muted,s==='完成'?C.greenSoft:s==='阻塞'?C.orangeSoft:s==='执行中'||s==='进行中'?C.orangeSoft:C.side,72)+txt(x+12,y+85,i===0?'前置 1 · 后续 2':'子需求 0',8,C.faint,500);});});
  c+=txt(970,180,'模型路由矩阵',20,C.ink,650)+chip(1450,162,'执行中',C.orange,C.orangeSoft,76)+txt(970,212,'Feature · High priority · 前置：模型设置重构',9,C.muted,450);
  c+=txt(970,264,'Requirement',9,C.ink,700)+txt(1060,264,'Work items',9,C.muted,500)+txt(1150,264,'Documents',9,C.muted,500)+line(970,282,1568,282,C.line2);
  c+=rows(970,322,['将模型、供应商、默认路由与回退策略整合到同一工作面。','配置来源必须明确区分 Workspace、User 和 Environment。'],11,C.text,24,450);
  c+=txt(970,414,'Work items',10,C.faint,700);[['建立 ModelMatrix 组件','完成',C.green],['实现 FallbackChain','进行中',C.orange],['迁移密钥入口','阻塞',C.orange],['补充键盘导航','等待',C.faint]].forEach(([n,s,color],i)=>{const y=450+i*56;c+=circle(976,y-3,4,color)+txt(990,y,n,10,C.text,600)+chip(1478,y-20,s,color,color===C.green?C.greenSoft:color===C.orange?C.orangeSoft:C.side,76)+line(990,y+26,1568,y+26,C.line2);});
  c+=rect(970,714,598,154,C.pane,C.line2,9)+txt(990,744,'Execution',10,C.ink,650)+txt(990,773,'已生成任务 DAG，当前有 1 个阻塞节点。',9,C.muted,450)+button(990,808,'打开执行工作台','dark',188)+button(1190,808,'重新生成计划','default',166);
  return doc('ChatOS Project Plan Workspace Revision 03',shell(c,{title:'ChatOS / Project',repo:'Plan · 12 requirements',status:'运行中'}));
}

function projectTeam(){
  let c=rect(272,92,1328,908,C.bg)+rect(272,92,1328,48,C.paper)+txt(300,122,'Files',9,C.muted,500)+txt(354,122,'Team',9,C.ink,700)+txt(408,122,'Plan',9,C.muted,500)+txt(456,122,'Settings',9,C.muted,500)+line(344,138,386,138,C.orange,2)+line(272,140,1600,140,C.line);
  c+=rect(292,160,1288,70,C.paper,C.line2,9)+txt(312,187,'Project contact',8,C.faint,700)+circle(314,207,4,C.green)+txt(328,211,'Product Agent · 项目默认联系人',10,C.text,600)+button(1430,177,'更改联系人','default',126);
  c+=rect(292,250,320,730,C.side)+rect(612,250,610,730,C.paper)+rect(1222,250,358,730,C.pane)+line(612,250,612,980,C.line)+line(1222,250,1222,980,C.line);
  c+=txt(314,282,'User messages',11,C.ink,650)+txt(586,282,'Summary',8,C.blue,650,'end');
  [['重新设计模型配置页','今天 10:39',true],['请检查权限边界','昨天',false],['启动项目并验证','周一',false]].forEach(([n,t,a],i)=>{const y=310+i*88;if(a)c+=rect(304,y,296,74,C.paper,'none',8);c+=circle(322,y+20,4,a?C.orange:C.faint)+txt(338,y+23,n,10,C.text,a?650:500)+txt(338,y+46,t,8,C.faint,450)+chip(518,y+13,a?'3 tasks':'完成',a?C.orange:C.green,a?C.orangeSoft:C.greenSoft,68);});
  c+=txt(640,282,'Product Agent',11,C.ink,650)+txt(1194,282,'ChatOS · main',8,C.faint,500,'end',mono)+line(634,300,1200,300,C.line2);
  c+=txt(652,344,'项目成员工作区',20,C.ink,650)+rows(652,380,['这里保留项目联系人会话、锚定用户消息和任务入口。','成员可以从一条消息直接打开对应任务 DAG。'],11,C.muted,22,450)+rect(652,454,516,90,C.bg,C.line2,9)+txt(670,482,'已锚定消息',8,C.orange,700)+txt(670,512,'重新设计模型配置页',11,C.ink,600)+button(1030,481,'打开任务图','dark',120);
  c+=composer(646,820,548,'向 Product Agent 发送消息…');
  c+=txt(1246,282,'Runtime context',11,C.ink,650);[['Agent','Product Agent'],['Project','ChatOS'],['Branch','main'],['Permission','Workspace'],['Model','GPT-5']].forEach(([a,b],i)=>{const y=326+i*42;c+=txt(1246,y,a,8,C.faint,650)+txt(1556,y,b,8,a==='Permission'?C.purple:C.text,600,'end',a==='Branch'?mono:sans);});
  c+=line(1246,540,1556,540,C.line2)+txt(1246,574,'Message actions',9,C.faint,700)+button(1246,598,'Session summary','default',310)+button(1246,646,'Review repair','default',310)+button(1246,694,'Open task graph','dark',310);
  return doc('ChatOS Project Team Revision 03',shell(c,{title:'ChatOS / Team',repo:'Product Agent · project contact',status:'空闲'}));
}

function projectRunSettings(){
  let c=rect(272,92,1328,908,C.bg)+rect(272,92,1328,48,C.paper)+txt(300,122,'Files',9,C.muted,500)+txt(354,122,'Team',9,C.muted,500)+txt(408,122,'Plan',9,C.muted,500)+txt(456,122,'Settings',9,C.ink,700)+line(448,138,500,138,C.orange,2)+line(272,140,1600,140,C.line);
  c+=rect(310,166,1252,792,C.paper,C.line,11)+txt(336,204,'ChatOS runtime',17,C.ink,650)+txt(336,229,'/project/chatos_rs',9,C.faint,500,'start',mono)+chip(1382,184,'READY',C.green,C.greenSoft,78)+chip(1470,184,'3 targets',C.muted,C.side,76);
  c+=txt(336,282,'Preflight',9,C.faint,700)+rect(336,300,574,86,C.greenSoft,'none',9)+circle(356,327,5,C.green)+txt(372,331,'环境检查通过',10,C.green,650)+txt(372,356,'Rust、Node、pnpm 与配置文件均可用',9,C.text,450);
  c+=txt(336,430,'Run target',9,C.faint,700)+rect(336,448,574,48,C.paper,C.line,8)+txt(352,478,'Frontend · Vite · localhost:8088',10,C.ink,600)+txt(888,478,'⌄',10,C.muted,550,'end');
  c+=txt(336,538,'Toolchains',9,C.faint,700);[['Node','v22.14','Detected',C.green],['pnpm','10.4','Detected',C.green],['Rust','1.86','Detected',C.green]].forEach(([a,b,s,color],i)=>{const y=568+i*48;c+=rect(336,y,574,38,C.pane,C.line2,7)+txt(350,y+24,a,9,C.ink,650)+txt(500,y+24,b,8,C.muted,550,'start',mono)+txt(892,y+24,s,8,color,650,'end');});
  c+=txt(336,746,'Environment variables',9,C.faint,700)+rect(336,764,574,92,C.pane,C.line2,8)+txt(352,790,'PORT=8088',9,C.text,500,'start',mono)+txt(352,816,'CHATOS_ENV=local',9,C.text,500,'start',mono)+button(336,878,'Start new','accent',128)+button(474,878,'Restart','default',110)+button(594,878,'Stop','default',100);
  c+=txt(956,282,'Command preview',9,C.faint,700)+rect(956,300,574,128,C.terminal,'none',9)+txt(978,334,'$ pnpm --dir chatos/frontend dev',10,C.terminalText,500,'start',mono)+txt(978,367,'cwd  /project/chatos_rs',8,'#AEB4AA',500,'start',mono)+txt(978,395,'env  PORT=8088',8,'#AEB4AA',500,'start',mono);
  c+=txt(956,478,'Instances',9,C.faint,700);[['frontend #3','Running','12m',C.green],['frontend #2','Exited','42m',C.faint]].forEach(([a,b,t,color],i)=>{const y=510+i*58;c+=rect(956,y,574,48,C.pane,C.line2,8)+circle(974,y+24,4,color)+txt(988,y+27,a,9,C.ink,600)+txt(1438,y+27,b,8,color,650,'end')+txt(1512,y+27,t,8,C.faint,500,'end',mono);});
  c+=txt(956,662,'Live terminal',9,C.faint,700)+rect(956,680,574,222,C.terminal,'none',9)+rows(978,715,['> chatos@dev','> vite --host 0.0.0.0','Local: http://localhost:8088/','ready in 640ms'],9,C.terminalText,29,500,mono);
  return doc('ChatOS Project Runtime Settings Revision 03',shell(c,{title:'ChatOS / Settings',repo:'Run environment',status:'运行中'}));
}

function remoteWorkspace(){
  let c=rect(272,92,860,908,C.terminal)+rect(1132,92,468,908,C.pane)+line(1132,92,1132,1000,C.line);
  c+=rect(272,92,860,46,C.terminal2)+circle(294,115,5,'#CA8175')+circle(312,115,5,'#CAA25D')+circle(330,115,5,'#6FA189')+txt(354,119,'production · ssh',9,'#AEB4AA',550,'start',mono)+chip(980,102,'CONNECTED',C.terminalText,'#374239',110);
  rows(304,184,['$ ssh deploy@10.0.2.14','$ cd /srv/chatos','$ docker compose ps','chatos-api      Up 18 hours','task-runner     Up 18 hours','mongo           Up 5 days','','$ tail -f logs/chatos.log','[info] connector heartbeat ok','[info] active task runs: 1','','$ _'],10,C.terminalText,43,500,mono).split('');
  const terms=[['$ ssh deploy@10.0.2.14',C.orange],['$ cd /srv/chatos',C.orange],['$ docker compose ps',C.orange],['chatos-api      Up 18 hours','#83BE96'],['task-runner     Up 18 hours','#83BE96'],['mongo           Up 5 days','#83BE96'],['',''],['$ tail -f logs/chatos.log',C.orange],['[info] connector heartbeat ok','#AEB4AA'],['[info] active task runs: 1','#AEB4AA'],['',''],['$ _',C.orange]];terms.forEach(([s,color],i)=>c+=txt(304,184+i*43,s,10,color||C.terminalText,500,'start',mono));
  c+=txt(1160,130,'Remote resources',11,C.ink,650)+button(1466,108,'新建连接','default',108,'new')+line(1156,150,1574,150,C.line2);
  [['Production','10.0.2.14','Connected',C.green],['Staging','10.0.2.21','Offline',C.faint]].forEach(([n,a,s,color],i)=>{const y=184+i*64;c+=circle(1162,y-2,4,color)+txt(1176,y,n,10,C.ink,650)+txt(1176,y+22,a,8,C.faint,500,'start',mono)+txt(1572,y,s,8,color,650,'end');});
  c+=txt(1160,350,'SFTP',11,C.ink,650)+line(1156,370,1574,370,C.line2)+txt(1160,404,'/srv/chatos',9,C.text,600,'start',mono);
  [['logs','folder','—'],['releases','folder','—'],['docker-compose.yml','file','3 KB'],['.env.production','file','1 KB']].forEach(([n,ico,size],i)=>{const y=438+i*45;c+=icon(ico,1160,y-15,16,C.muted)+txt(1184,y,n,9,C.text,550,'start',mono)+txt(1572,y,size,8,C.faint,500,'end');});
  c+=rect(1160,658,412,142,C.paper,C.line2,9)+txt(1180,688,'Transfer queue',10,C.ink,650)+txt(1180,722,'release-2026-08-21.tar.gz',8,C.text,550,'start',mono)+txt(1548,722,'68%',8,C.blue,650,'end')+rect(1180,742,368,7,C.side,'none',4)+rect(1180,742,250,7,C.blue,'none',4)+button(1160,834,'返回终端','dark',412);
  return doc('ChatOS Remote Workspace Revision 03',shell(c,{title:'Remote / Production',repo:'SSH + SFTP',status:'运行中'}));
}

function userSettings(){
  let c=rect(272,92,218,908,C.side)+rect(490,92,1110,908,C.bg)+line(490,92,490,1000,C.line);
  c+=txt(300,132,'User settings',16,C.ink,650);[['General',true],['Cloud AI',false]].forEach(([n,a],i)=>{const y=176+i*44;if(a)c+=rect(288,y-23,190,35,C.paper,'none',8);c+=txt(306,y,n,10,a?C.ink:C.muted,a?650:500);});
  c+=pageHead(536,154,'General','控制语言、主题、消息行为与本地工作区偏好。',button(1422,126,'保存设置','dark',130));
  const sections=[['Appearance',[['Theme','System'],['Language','简体中文'],['Density','Comfortable']]],['Conversation',[['Auto-scroll','On'],['Show tool details','Collapsed'],['Enter to send','On']]],['Workspace',[['Restore last session','On'],['Default environment','Local'],['Confirm destructive actions','Always']]]];
  sections.forEach(([title,items],si)=>{const y=226+si*222;c+=txt(536,y,title,11,C.ink,650)+rect(536,y+20,1018,174,C.paper,C.line,10);items.forEach(([a,b],i)=>{const yy=y+56+i*46;c+=txt(558,yy,a,10,C.text,550)+txt(1528,yy,b,9,i===2&&si===2?C.orange:C.muted,600,'end')+(i<2?line(558,yy+20,1528,yy+20,C.line2):'');});});
  return doc('ChatOS User Settings Revision 03',shell(c,{title:'User settings',repo:'Lee / preferences',status:'未保存'}));
}

function applicationsManage(){
  let c=rect(272,92,1328,908,C.bg)+pageHead(326,158,'Applications','浏览可用应用，并管理工作区已经安装的连接。',button(1430,126,'添加应用','dark',124,'new'));
  c+=rect(326,212,1216,42,C.paper,C.line,9)+txt(346,238,'Browse',10,C.muted,550)+txt(420,238,'Manage',10,C.ink,700)+line(412,252,468,252,C.orange,2)+icon('search',1496,223,16,C.muted);
  c+=txt(326,300,'Installed',11,C.ink,650)+txt(1528,300,'5 applications',8,C.faint,500,'end');
  const apps=[['Local Files','Read and edit workspace files','Workspace','Connected',C.green],['Project Manager','Requirements, plans and work items','Workspace','Connected',C.green],['Browser','Open pages and inspect previews','Task','Available',C.blue],['GitHub','Repositories, issues and pull requests','User','Needs auth',C.orange],['Slack','Search and send workspace messages','User','Disabled',C.faint]];
  apps.forEach(([n,d,scope,status,color],i)=>{const y=326+i*104;c+=rect(326,y,1216,88,C.paper,C.line2,10)+rect(346,y+18,48,48,C.bg,C.line2,9)+icon(i===0?'folder':i===1?'plan':i===2?'apps':'spark',360,y+32,20,color)+txt(414,y+30,n,11,C.ink,650)+txt(414,y+54,d,9,C.muted,450)+chip(1178,y+29,scope,C.muted,C.side,92)+circle(1320,y+42,4,color)+txt(1334,y+46,status,9,color,650)+txt(1518,y+46,'•••',12,C.muted,600,'end');});
  c+=rect(326,874,1216,54,C.pane,C.line2,9)+txt(346,906,'应用权限在任务启动时再次确认，安装本身不会扩大当前任务权限。',9,C.muted,500)+txt(1518,906,'Plugin policy →',9,C.blue,650,'end');
  return doc('ChatOS Applications Manage Revision 03',shell(c,{mode:'apps',title:'Applications',repo:'Workspace integrations',status:'空闲'}));
}

function creationFlows(){
  let b=windowBar()+logo(54,68,32,true)+txt(54,138,'Creation flows',30,C.ink,650)+txt(54,168,'所有资源创建采用同一种两段式工作流：必要信息 → 验证与确认。',11,C.muted,450);
  const dialogs=[['New task',['Project','Agent','Environment','Permission'],'Create task',C.orange],['New project',['Name','Local directory','Default branch'],'Create project',C.green],['New terminal',['Name','Working directory','Shell'],'Create terminal',C.blue],['Remote connection',['Host','Port','Username','Authentication'],'Verify & connect',C.purple],['New contact',['Display name','Agent','Project scope'],'Create contact',C.orange],['Install application',['Application','Scope','Requested capabilities'],'Review permissions',C.green]];
  dialogs.forEach(([title,fields,action,color],i)=>{const col=i%3,row=Math.floor(i/3),x=54+col*510,y=210+row*372;b+=rect(x,y,474,334,C.paper,C.line,13,{filter:'url(#lift)'})+txt(x+22,y+34,title,14,C.ink,650)+txt(x+450,y+34,'×',13,C.muted,500,'end')+line(x+18,y+52,x+456,y+52,C.line2);fields.forEach((f,fi)=>{const yy=y+82+fi*53;b+=txt(x+22,yy,f,8,C.faint,650)+rect(x+22,yy+10,430,34,C.paper,C.line,7)+txt(x+34,yy+31,fi===0?'Select or enter…':'—',8,C.faint,450);});b+=line(x+18,y+276,x+456,y+276,C.line2)+button(x+250,y+290,'Cancel','default',90)+rect(x+350,y+290,102,36,color,color,9)+txt(x+401,y+313,action,9,C.paper,650,'middle');});
  b+=txt(54,952,'Confirmation is required before credentials, external connections, destructive actions, or permission expansion.',9,C.faint,550);
  return doc('ChatOS Creation Flows Revision 03',b);
}

function statesBoard(){
  let b=windowBar()+logo(54,68,32,true)+txt(54,138,'Product states',30,C.ink,650)+txt(54,168,'空白、加载、失败和权限状态必须与正常界面一样完整。',11,C.muted,450);
  const cards=[['Empty session','还没有会话','从一个结果明确的任务开始。','新建任务',C.faint],['Loading graph','正在生成任务流程图','读取依赖并计算布局…','',C.blue],['Offline connector','Local Connector 已断开','本地文件与终端暂时不可用。','重新连接',C.orange],['Permission request','需要 Workspace 权限','任务将编辑 3 个文件并运行测试。','允许一次',C.purple],['Task failed','节点执行失败','测试未通过，后续任务已暂停。','打开详情',C.orange],['No project','尚未选择项目','选择目录后才能使用文件与运行能力。','选择项目',C.green]];
  cards.forEach(([title,head,desc,action,color],i)=>{const col=i%3,row=Math.floor(i/3),x=54+col*510,y=220+row*340;b+=rect(x,y,474,296,C.paper,C.line,12)+circle(x+237,y+78,24,color==='none'?C.side:color+'22',color,1.5)+icon(i===1?'clock':i===3?'settings':i===4?'terminal':i===5?'folder':'chat',x+227,y+68,20,color)+txt(x+237,y+130,title,9,C.faint,700,'middle',mono)+txt(x+237,y+164,head,15,C.ink,650,'middle')+txt(x+237,y+190,desc,9,C.muted,450,'middle');if(action)b+=button(x+167,y+220,action,i===3?'dark':'default',140);else{b+=circle(x+215,y+238,4,C.blue)+circle(x+237,y+238,4,C.blue)+circle(x+259,y+238,4,C.blue);}});
  b+=txt(54,938,'State language: explain what happened · what is unavailable · what the user can do next.',9,C.faint,550);
  return doc('ChatOS Product States Revision 03',b);
}

function sessionContext(){
  let c=rect(272,92,738,908,C.paper)+rect(1010,92,590,908,C.pane)+line(1010,92,1010,1000,C.line);
  c+=txt(322,136,'Session summary',9,C.faint,700)+txt(322,170,'模型配置页重构',20,C.ink,650)+txt(322,198,'12 messages · 3 tasks · 2 files changed',9,C.muted,450)+line(310,222,980,222,C.line2);
  c+=txt(322,264,'Current objective',10,C.ink,650)+rows(322,294,['重组模型、供应商、默认路由与配置来源，','不修改后端接口。'],12,C.text,22,450);
  c+=txt(322,372,'Confirmed decisions',10,C.ink,650);[['模型路由作为主结构','Task #184'],['密钥归入连接设置','Product decision'],['回退链显式展示','Runtime policy']].forEach(([n,s],i)=>{const y=410+i*58;c+=circle(328,y-3,4,i===0?C.orange:i===1?C.blue:C.green)+txt(342,y,n,10,C.text,600)+txt(958,y,s,8,C.faint,500,'end');});
  c+=txt(322,616,'Open items',10,C.ink,650);[['确认密钥迁移范围',C.orange],['完成键盘导航验证',C.faint]].forEach(([n,color],i)=>{const y=654+i*47;c+=circle(328,y-3,4,color)+txt(342,y,n,10,C.text,550);});
  c+=rect(322,790,640,116,C.bg,C.line2,9)+txt(342,820,'Summary health',9,C.ink,650)+txt(342,850,'最近更新 2 分钟前 · 无待修复摘要',9,C.muted,450)+chip(830,808,'HEALTHY',C.green,C.greenSoft,104);
  c+=txt(1038,130,'Runtime context',11,C.ink,650)+txt(1568,130,'Turn 18',8,C.faint,550,'end',mono)+line(1036,150,1574,150,C.line2);
  [['Agent','Code Agent',C.green],['Project','ChatOS',C.text],['Branch','codex/ui-r3',C.text],['Environment','Local',C.green],['Permission','Workspace',C.purple],['Model','GPT-5',C.blue]].forEach(([a,b,color],i)=>{const y=194+i*48;c+=txt(1038,y,a,8,C.faint,650)+txt(1568,y,b,9,color,600,'end',a==='Branch'?mono:sans);});
  c+=txt(1038,514,'Context sources',10,C.ink,650);[['System context','1.8k'],['Project memory','8.4k'],['Conversation','16.2k'],['Attached files','2.0k']].forEach(([a,b],i)=>{const y=550+i*39;c+=txt(1038,y,a,9,C.text,550)+txt(1568,y,b,8,C.faint,500,'end',mono);});
  c+=rect(1038,740,530,142,C.paper,C.line2,9)+txt(1058,770,'Context budget',10,C.ink,650)+txt(1548,770,'24%',9,C.blue,650,'end')+rect(1058,794,490,8,C.side,'none',4)+rect(1058,794,118,8,C.blue,'none',4)+txt(1058,828,'28.4k / 118k tokens',8,C.muted,500,'start',mono)+txt(1058,854,'自动压缩：开启',8,C.green,600);
  return doc('ChatOS Session Summary Runtime Context Revision 03',shell(c,{title:'Session context',repo:'模型配置页重构 / Turn 18',status:'已保存'}));
}

const out=[
  ['00-overview-board.svg',overview()],['01-login.svg',login()],['02-command-center.svg',hub()],['03-agent-chat.svg',chat()],
  ['04-project-workspace.svg',files()],['05-project-plan.svg',plan()],['06-runtime-terminal.svg',runtime()],['07-ai-settings.svg',settings()],
  ['08-agents-apps.svg',agents()],['09-notepad.svg',notes()],['10-system-context.svg',contextEditor()],
  ['11-design-system.svg',designSystem()],['12-task-blocked.svg',taskBlocked()],['13-task-run-detail.svg',taskRunDetail()],
  ['14-project-plan-workspace.svg',projectPlanWorkspace()],['15-project-team.svg',projectTeam()],['16-project-runtime-settings.svg',projectRunSettings()],
  ['17-remote-workspace.svg',remoteWorkspace()],['18-user-settings.svg',userSettings()],['19-applications-manage.svg',applicationsManage()],
  ['20-creation-flows.svg',creationFlows()],['21-product-states.svg',statesBoard()],['22-session-summary-context.svg',sessionContext()]
];
fs.mkdirSync(OUT,{recursive:true});
for(const [name,svg] of out)fs.writeFileSync(path.join(OUT,name),svg+'\n','utf8');
console.log(`Generated ${out.length} Revision 03 SVG screens in ${OUT}`);
