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

function defs(){return `<defs><filter id="shadow" x="-30%" y="-30%" width="160%" height="180%"><feDropShadow dx="0" dy="10" stdDeviation="22" flood-color="#20211F" flood-opacity=".10"/></filter><filter id="lift" x="-30%" y="-30%" width="160%" height="180%"><feDropShadow dx="0" dy="4" stdDeviation="10" flood-color="#20211F" flood-opacity=".08"/></filter></defs>`}
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
  c+=line(1174,500,1572,500,C.line2)+[['来源','Workspace',C.purple],['失败策略','自动回退',C.green],['权限','按任务决定',C.orange]].forEach(([a,b,color],i)=>{const y=536+i*37;c+=txt(1174,y,a,9,C.faint,650)+txt(1572,y,b,9,color,650,'end');});
  c+=button(1174,668,'应用更改','dark',398)+txt(1174,754,'Review',10,C.ink,650)+[['类型检查通过',C.green],['组件测试通过',C.green],['等待视觉确认',C.orange]].forEach(([n,color],i)=>{const y=784+i*35;c+=circle(1178,y-3,4,color)+txt(1192,y,n,9,C.text,520);});
  return doc('ChatOS Files Revision 03',shell(c,{status:'已保存',actions:button(1428,47,'提交','default',142)}));
}

function plan(){
  let c=rect(272,92,872,908,C.paper)+rect(1144,92,456,908,C.pane)+line(1144,92,1144,1000,C.line);
  c+=pageHead(326,154,'Project plan','设置页信息架构重构',chip(950,120,'5 / 8 steps',C.orange,C.orangeSoft,102));
  c+=line(318,202,1110,202,C.line2)+txt(326,240,'Execution path',10,C.faint,700);
  const steps=[['01','梳理现有配置来源','完成','done'],['02','建立模型与供应商矩阵','完成','done'],['03','定义默认与回退策略','完成','done'],['04','实现设置页新布局','Code Agent 正在修改 AISettings.vue','run'],['05','补充权限与来源提示','待开始','todo'],['06','视觉与键盘操作验证','待开始','todo']];
  steps.forEach(([n,t,s,state],i)=>{const y=286+i*94;const color=state==='done'?C.green:state==='run'?C.orange:C.line;c+=circle(348,y,15,state==='todo'?C.paper:color,color,1.5)+txt(348,y+3,state==='done'?'✓':n,8,state==='todo'?C.faint:C.paper,700,'middle',mono);if(i<steps.length-1)c+=line(348,y+16,348,y+78,C.line,2);c+=txt(382,y-2,t,12,C.ink,620)+txt(382,y+23,s,9,state==='run'?C.orange:state==='done'?C.green:C.faint,500);if(state==='run')c+=rect(760,y-20,332,58,C.orangeSoft,'none',9)+txt(778,y+2,'正在编辑',8,C.orange,700)+txt(778,y+23,'AISettings.vue · 2 files changed',9,C.text,520);});
  c+=txt(1170,130,'Decisions',11,C.ink,650)+line(1168,150,1574,150,C.line2);
  [['模型路由优先单屏可读','产品 · 今天',C.blue],['密钥管理归入连接设置','架构 · 昨天',C.purple],['回退链必须显式展示','运行时 · 昨天',C.green]].forEach(([n,m,color],i)=>{const y=192+i*77;c+=circle(1176,y-2,4,color)+txt(1190,y+2,n,10,C.ink,600)+txt(1190,y+24,m,8,C.faint,450);if(i<2)c+=line(1190,y+45,1574,y+45,C.line2);});
  c+=txt(1170,450,'Verification',11,C.ink,650)+line(1168,470,1574,470,C.line2);
  [['类型检查','通过',C.green],['组件测试','通过',C.green],['键盘导航','待验证',C.orange],['视觉回归','待确认',C.orange]].forEach(([a,b,color],i)=>{const y=508+i*49;c+=txt(1172,y,a,10,C.text,520)+chip(1480,y-19,b,color,color===C.green?C.greenSoft:C.orangeSoft,82);});
  c+=rect(1170,738,404,150,C.paper,C.line2,10)+txt(1190,770,'Plan stays connected',10,C.ink,650)+rows(1190,800,['每个步骤都能回到对应会话、','文件变更与运行记录。'],9,C.muted,20,450)+txt(1550,857,'AUTO SYNC',8,C.green,700,'end',mono);
  return doc('ChatOS Plan Revision 03',shell(c,{title:'Project plan',status:'运行中',actions:button(1428,47,'编辑计划','default',142)}));
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
  c+=txt(1212,756,'Recent runs',11,C.ink,650)+line(1210,776,1574,776,C.line2)+[['UI generator','18s',C.green],['SVG validation','1.2s',C.green],['Visual QA','waiting',C.orange]].forEach(([a,b,color],i)=>{const y=816+i*42;c+=circle(1216,y-3,4,color)+txt(1230,y,a,9,C.text,550)+txt(1572,y,b,8,C.faint,500,'end',mono);});
  return doc('ChatOS Runtime Revision 03',shell(c,{title:'Runtime',repo:'chatos-rs / codex-ui-r3',actions:button(1428,47,'启动全部','accent',142)}));
}

function settings(){
  let c=rect(272,92,218,908,C.side)+rect(490,92,1110,908,C.bg)+line(490,92,490,1000,C.line);
  c+=txt(300,132,'Settings',16,C.ink,650)+[['个人',false],['连接',false],['AI 与模型',true],['权限',false],['记忆',false],['快捷键',false]].forEach(([n,a],i)=>{const y=174+i*45;if(a)c+=rect(288,y-23,190,35,C.blueSoft,'none',8);c+=txt(306,y,n,10,a?C.blue:C.muted,a?650:500);});
  c+=pageHead(536,154,'AI 与模型','管理模型可用性与任务路由。密钥归连接设置管理。',button(1426,126,'保存更改','dark',126));
  c+=txt(536,222,'Default route',10,C.faint,700)+rect(536,240,1018,72,C.paper,C.line,11)+chip(558,263,'GPT-5',C.blue,C.blueSoft,72)+txt(642,280,'→',11,C.faint,650)+chip(674,263,'Claude Sonnet',C.purple,C.purpleSoft,118)+txt(804,280,'→',11,C.faint,650)+chip(836,263,'Local Qwen',C.green,C.greenSoft,102)+txt(1528,283,'编辑路由',9,C.blue,650,'end');
  c+=txt(536,366,'Available models',12,C.ink,650)+txt(1554,366,'3 providers · 5 models',8,C.faint,550,'end');
  c+=rect(536,388,1018,330,C.paper,C.line,11);
  const xs=[558,830,1030,1165,1320,1518];['模型','来源','上下文','工具','角色','启用'].forEach((n,i)=>c+=txt(xs[i],420,n,8,C.faint,700,i===5?'end':'start'));
  const m=[['GPT-5','Cloud AI','128k','完整','默认',C.blue,true],['Claude Sonnet','Anthropic','200k','完整','回退 1',C.purple,true],['Local Qwen','Local','32k','受限','回退 2',C.green,true],['Gemini Pro','Cloud AI','1m','完整','—',C.blue,false],['DeepSeek R1','Local','64k','受限','—',C.green,false]];
  m.forEach(([n,src,ctx,tool,role,color,on],i)=>{const y=466+i*50;if(i%2)c+=rect(537,y-24,1016,48,C.pane);c+=circle(560,y-3,4,color)+txt(574,y,n,9,C.ink,650)+txt(830,y,src,8,C.muted,500)+txt(1030,y,ctx,8,C.text,520,'start',mono)+txt(1165,y,tool,8,C.muted,500)+txt(1320,y,role,8,role==='—'?C.faint:color,650)+rect(1480,y-15,38,21,on?color:C.side,'none',11)+circle(on?1507:1491,y-5,8,C.paper);});
  c+=rect(536,746,498,180,C.paper,C.line,11)+txt(558,778,'Configuration source',11,C.ink,650)+[['Workspace','优先',C.purple],['User preferences','继承',C.muted],['Environment','3 variables',C.muted]].forEach(([a,b,color],i)=>{const y=818+i*32;c+=txt(558,y,a,8,color,600)+txt(1012,y,b,8,C.faint,500,'end');});
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
  c+=txt(1290,322,'Composition',10,C.ink,650)+[['Base','全局规则',C.purple],['Agent','角色能力',C.blue],['Project','项目约束',C.green],['Task','当前指令',C.orange]].forEach(([a,b,color],i)=>{const y=356+i*52;c+=rect(1288,y,264,40,C.paper,C.line2,8)+circle(1304,y+20,4,color)+txt(1318,y+23,a,9,C.ink,650)+txt(1534,y+23,b,7,C.faint,500,'end');});
  c+=txt(1290,608,'Publish checks',10,C.ink,650)+[['变量语法','通过',C.green],['冲突规则','通过',C.green],['权限扩张','无',C.green],['Token 估算','1.8k',C.blue],['测试会话','待运行',C.orange]].forEach(([a,b,color],i)=>{const y=650+i*43;c+=txt(1290,y,a,8,C.muted,550)+txt(1534,y,b,8,color,650,'end');});
  return doc('ChatOS System Context Revision 03',shell(c,{active:'Agent 角色系统',title:'System context',repo:'Code Agent / draft v13',status:'草稿'}));
}

const out=[
  ['00-overview-board.svg',overview()],['01-login.svg',login()],['02-command-center.svg',hub()],['03-agent-chat.svg',chat()],
  ['04-project-workspace.svg',files()],['05-project-plan.svg',plan()],['06-runtime-terminal.svg',runtime()],['07-ai-settings.svg',settings()],
  ['08-agents-apps.svg',agents()],['09-notepad.svg',notes()],['10-system-context.svg',contextEditor()]
];
fs.mkdirSync(OUT,{recursive:true});
for(const [name,svg] of out)fs.writeFileSync(path.join(OUT,name),svg+'\n','utf8');
console.log(`Generated ${out.length} Revision 03 SVG screens in ${OUT}`);
