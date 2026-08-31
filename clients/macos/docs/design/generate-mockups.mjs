import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outDir = path.join(root, 'screens');
fs.mkdirSync(outDir, { recursive: true });

const C = {
  bg: '#EDEDF1', window: '#FBFBFC', sidebar: '#F4F4F6', panel: '#FFFFFF',
  text: '#202124', secondary: '#6E6E73', tertiary: '#9A9AA1', line: '#D9D9DE',
  blue: '#0A7AFF', blueSoft: '#E7F1FF', green: '#2EA44F', greenSoft: '#EAF7EE',
  orange: '#D97720', orangeSoft: '#FFF2E5', red: '#D84A4A', redSoft: '#FCEBEC',
  purple: '#7057D8', purpleSoft: '#F0ECFF', code: '#F7F7F9', dark: '#22262D',
};

const font = `-apple-system,BlinkMacSystemFont,'SF Pro Display','PingFang SC','Helvetica Neue',Arial,sans-serif`;
const mono = `'SFMono-Regular','SF Mono',Menlo,Consolas,monospace`;
const esc = (v) => String(v).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
const rect = (x, y, w, h, fill = 'none', rx = 0, stroke = 'none', sw = 1, opacity = 1) => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${rx}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" opacity="${opacity}"/>`;
const line = (x1, y1, x2, y2, stroke = C.line, sw = 1, dash = '') => `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${stroke}" stroke-width="${sw}"${dash ? ` stroke-dasharray="${dash}"` : ''}/>`;
const circle = (cx, cy, r, fill, stroke = 'none', sw = 1) => `<circle cx="${cx}" cy="${cy}" r="${r}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}"/>`;
const text = (x, y, value, size = 14, weight = 400, fill = C.text, anchor = 'start', family = font) => `<text x="${x}" y="${y}" font-family="${family}" font-size="${size}" font-weight="${weight}" fill="${fill}" text-anchor="${anchor}">${esc(value)}</text>`;
const pathEl = (d, stroke = C.secondary, sw = 1.8, fill = 'none', dash = '') => `<path d="${d}" fill="${fill}" stroke="${stroke}" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"${dash ? ` stroke-dasharray="${dash}"` : ''}/>`;
const card = (x, y, w, h, inner = '', fill = C.panel, rx = 12, stroke = C.line) => `${rect(x, y, w, h, fill, rx, stroke)}${inner}`;
const pill = (x, y, label, fill = C.blueSoft, color = C.blue, width = null) => {
  const w = width ?? Math.max(48, label.length * 12 + 22);
  return `${rect(x, y, w, 24, fill, 12)}${text(x + w / 2, y + 16.5, label, 11, 600, color, 'middle')}`;
};
const button = (x, y, label, primary = false, width = null) => {
  const w = width ?? Math.max(76, label.length * 13 + 26);
  return `${rect(x, y, w, 32, primary ? C.blue : C.panel, 8, primary ? C.blue : C.line)}${text(x + w / 2, y + 21, label, 12, 600, primary ? '#FFF' : C.text, 'middle')}`;
};
const icon = (x, y, glyph, fill = C.blueSoft, color = C.blue, size = 32) => `${rect(x, y, size, size, fill, 8)}${text(x + size / 2, y + size * .68, glyph, size * .46, 650, color, 'middle')}`;
const traffic = () => `${circle(26, 25, 6, '#FF5F57')}${circle(46, 25, 6, '#FEBB2E')}${circle(66, 25, 6, '#28C840')}`;

function write(name, body) {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1440" height="900" viewBox="0 0 1440 900"><defs><filter id="shadow" x="-10%" y="-10%" width="120%" height="130%"><feDropShadow dx="0" dy="12" stdDeviation="18" flood-color="#000" flood-opacity=".13"/></filter></defs><rect width="1440" height="900" fill="${C.bg}"/><g filter="url(#shadow)">${body}</g></svg>`;
  fs.writeFileSync(path.join(outDir, name), svg);
}

function chrome(title = 'ChatOS') {
  return `${rect(0, 0, 1440, 900, C.window, 16)}${rect(0, 0, 1440, 48, '#F7F7F8', 16)}${rect(0, 32, 1440, 16, '#F7F7F8')}${line(0, 48, 1440, 48)}${traffic()}${text(100, 31, title, 13, 650)}`;
}

function resourceSidebar(active = 'test_project') {
  let s = rect(0, 48, 244, 852, C.sidebar) + line(244, 48, 244, 900);
  const activeTerminal = String(active).startsWith('terminal:') ? String(active).slice('terminal:'.length) : null;
  const sections = [
    ['联系人', [['叽咕狸', '20 天前 · 任务系统']]],
    ['项目', [['test_project', ''], ['nova_dreamer', ''], ['vrad-backend', '']]],
    ['终端', activeTerminal ? [[activeTerminal, '运行中 · 刚刚']] : [['还没有终端', '点击 + 新建']]],
    ['远端', [['还没有远端连接', '点击 + 新建']]],
  ];
  let y = 74;
  sections.forEach(([titleValue, items], sectionIndex) => {
    s += text(18, y, '⌄', 11, 650, C.secondary) + text(36, y, titleValue, 12, 650, C.secondary);
    s += text(210, y, sectionIndex === 0 || sectionIndex === 2 || sectionIndex === 3 ? '↻' : '', 12, 500, C.tertiary, 'middle');
    s += text(228, y, '+', 15, 500, C.secondary, 'middle');
    y += 18;
    items.forEach(([name, sub]) => {
      const isActive = name === active || (sectionIndex === 2 && name === activeTerminal);
      if (isActive) s += rect(10, y, 224, sub ? 44 : 34, C.blueSoft, 8);
      s += text(22, y + 21, sectionIndex === 0 ? '●' : sectionIndex === 1 ? '▱' : sectionIndex === 2 ? '>_' : '⇄', 12, 600, isActive ? C.blue : C.secondary);
      s += text(46, y + 20, name, 12, isActive ? 650 : 500, isActive ? C.blue : C.text);
      if (sub) s += text(46, y + 36, sub, 10, 400, C.tertiary);
      y += sub ? 50 : 38;
    });
    if (sectionIndex < sections.length - 1) {
      s += line(12, y + 3, 232, y + 3);
      y += 28;
    }
  });
  return s;
}

function appToolbar(titleValue) {
  return `${rect(244, 48, 1196, 54, '#FBFBFC')}${line(244, 102, 1440, 102)}${text(270, 81, '‹', 20, 500, C.secondary)}${text(298, 81, titleValue, 15, 680)}${text(1216, 81, '▱', 15, 550, C.secondary)}${text(1258, 81, '◐', 15, 550, C.secondary)}${circle(1302, 75, 15, '#E0E3E9')}${text(1302, 80, 'L', 11, 700, C.secondary, 'middle')}${text(1326, 79, '设计账号', 11, 500, C.secondary)}`;
}

function visualPreview(titleValue, targetValue, pluginID, kind = 'desktop') {
  const x = 1046, y = 606, w = 376;
  let s = rect(x, y, w, 276, '#16161B', 14, '#414149') + line(x, y + 50, x + w, y + 50, '#303038');
  s += icon(x + 10, y + 10, kind === 'browser' ? '◉' : '▣', '#2E2B51', '#B7B3FF', 30);
  s += text(x + 50, y + 23, titleValue, 12, 700, '#F7F7F8') + circle(x + 53, y + 37, 4, '#42D99B');
  s += text(x + 64, y + 40, targetValue, 10, 500, '#A7A7B0') + text(x + w - 22, y + 31, '⌄', 15, 650, '#AAAAB3', 'middle');
  s += rect(x + 1, y + 51, w - 2, 196, '#09090B');
  if (kind === 'browser') {
    s += rect(x + 28, y + 66, 320, 166, '#F7F7F8', 6) + rect(x + 28, y + 66, 320, 24, '#E9EAED', 6) + circle(x + 40, y + 78, 3, '#FF5F57') + circle(x + 51, y + 78, 3, '#FEBB2E') + circle(x + 62, y + 78, 3, '#28C840');
    s += rect(x + 80, y + 72, 210, 12, '#FFF', 6) + text(x + 92, y + 81, 'developer.apple.com', 7, 500, '#6E6E73');
    s += text(x + 52, y + 120, 'Design for macOS', 16, 720, '#202124') + text(x + 52, y + 144, 'Navigation, windows, settings and system behavior.', 8, 450, '#6E6E73');
    s += rect(x + 52, y + 164, 116, 42, '#E7F1FF', 7) + text(x + 64, y + 182, 'Human Interface', 8, 650, '#0A7AFF') + text(x + 64, y + 195, 'Guidelines', 8, 500, '#6E6E73');
  } else {
    s += rect(x + 22, y + 64, 332, 170, '#1F2028', 7) + rect(x + 22, y + 64, 92, 170, '#292A34', 7);
    ['收件箱', '项目笔记', '每日记录', '归档'].forEach((label, i) => s += text(x + 36, y + 92 + i * 26, label, 8, i === 1 ? 650 : 450, i === 1 ? '#C7C4FF' : '#B4B5BE'));
    s += text(x + 132, y + 92, '项目笔记', 12, 700, '#F5F5F7') + line(x + 132, y + 104, x + 334, y + 104, '#393A45');
    s += text(x + 132, y + 126, '整理范围', 8, 650, '#B7B3FF') + text(x + 132, y + 146, '按实际主题迁移 Markdown 笔记，', 8, 450, '#C8C9D0') + text(x + 132, y + 161, '原始文件保持只读。', 8, 450, '#C8C9D0');
  }
  s += line(x, y + 248, x + w, y + 248, '#303038') + text(x + 10, y + 266, pluginID, 9, 500, '#85858F') + text(x + w - 10, y + 266, '仅在本机显示', 9, 500, '#85858F', 'end');
  return s;
}

function projectTabs(active, right = '') {
  const tabs = ['项目目录', '用户消息', 'Plan', '项目设置'];
  let s = rect(244, 102, 1196, 48, C.panel) + line(244, 150, 1440, 150);
  tabs.forEach((tab, i) => {
    const x = 270 + i * 92;
    if (tab === active) s += rect(x - 8, 111, 82, 30, '#ECEDEF', 7);
    s += text(x + 33, 131, tab, 12, tab === active ? 650 : 500, tab === active ? C.text : C.secondary, 'middle');
  });
  return s + right;
}

function resourceShell(titleValue, activeResource, body, projectTab = null, tabRight = '') {
  let s = chrome('ChatOS');
  s += resourceSidebar(activeResource);
  s += appToolbar(titleValue);
  if (projectTab) s += projectTabs(projectTab, tabRight);
  s += body;
  return s;
}

function connectorShell(active, body) {
  let s = chrome('ChatOS · Local Connector');
  s += rect(0, 48, 1440, 62, '#FBFBFC') + line(0, 110, 1440, 110);
  s += icon(24, 63, '⇄', C.greenSoft, C.green, 34) + text(72, 78, 'ChatOS', 11, 600, C.secondary) + text(72, 96, 'Local Connector', 17, 700);
  s += button(1042, 64, '返回主页面', false, 104) + circle(1174, 80, 5, C.green) + text(1188, 84, 'LOCAL CORE · 连接正常', 11, 600, C.secondary) + text(1360, 84, '◐  ↻', 15, 550, C.secondary, 'middle');
  s += rect(0, 110, 236, 790, C.sidebar) + line(236, 110, 236, 900);
  s += text(22, 144, 'CONTROL CENTER', 10, 700, C.tertiary);
  const tabs = ['设备配对', '外挂程式', '本机终端', '模型配置', '命令审批', '运行与权限', '权限控制'];
  tabs.forEach((tab, i) => {
    const y = 164 + i * 48;
    if (tab === active) s += rect(12, y, 212, 38, C.blueSoft, 8);
    s += text(30, y + 24, ['⇄', '◇', '>_', '◉', '✓', '⚙', '▣'][i], 14, 600, tab === active ? C.blue : C.secondary);
    s += text(58, y + 24, tab, 13, tab === active ? 650 : 500, tab === active ? C.blue : C.text);
  });
  s += card(16, 540, 204, 74, `${text(34, 568, '✓', 16, 700, C.green)}${text(60, 566, '本机安全边界', 12, 650)}${text(60, 588, '敏感能力仅在设备内执行', 10, 400, C.secondary)}`, '#F8F8FA', 10);
  return s + body;
}

function pageIntro(x, eyebrow, titleValue, subtitle) {
  return `${text(x, 145, eyebrow, 10, 700, C.tertiary)}${text(x, 177, titleValue, 24, 720)}${text(x, 203, subtitle, 12, 400, C.secondary)}${line(x, 222, 1410, 222)}`;
}

// 01 登录
{
  let s = chrome('ChatOS');
  s += rect(0, 48, 720, 852, '#F1F4FA');
  s += circle(150, 196, 62, C.purpleSoft) + text(150, 216, '✦', 52, 650, C.purple, 'middle');
  s += text(92, 316, '把长期项目，变成', 34, 720) + text(92, 360, '可以持续推进的协作。', 34, 720);
  s += text(94, 410, '客户端工作区与本机能力全部原生呈现', 15, 450, C.secondary);
  [['联系人和项目保持上下文', '不是临时聊天窗口'], ['任务过程可追踪', '流程、阻塞与 Run 均可回看'], ['本机安全边界', '文件、终端、插件和审批留在设备内']].forEach((item, i) => {
    const y = 486 + i * 82;
    s += icon(94, y, ['●', '✓', '⇄'][i], [C.blueSoft, C.purpleSoft, C.greenSoft][i], [C.blue, C.purple, C.green][i], 38);
    s += text(148, y + 17, item[0], 14, 650) + text(148, y + 39, item[1], 12, 400, C.secondary);
  });
  s += text(878, 194, '登录', 30, 720) + text(878, 224, '使用 ChatOS 平台账号继续', 13, 400, C.secondary);
  s += text(878, 286, '邮箱', 12, 600) + rect(878, 300, 404, 44, C.panel, 9, C.line) + text(894, 328, 'name@example.com', 13, 400, C.tertiary);
  s += text(878, 382, '密码', 12, 600) + rect(878, 396, 404, 44, C.panel, 9, C.line) + text(894, 424, '••••••••••••', 16, 500, C.secondary);
  s += rect(878, 472, 404, 42, C.blue, 9) + text(1080, 499, '登录', 13, 650, '#FFF', 'middle');
  s += pill(878, 560, '开发环境 · 127.0.0.1', '#F1F1F4', C.secondary, 182);
  write('01-login.svg', s);
}

// 02 真实全局资源壳
{
  let body = rect(244, 102, 1196, 798, '#FAFAFB');
  body += text(820, 326, '选择一个资源开始', 25, 720, C.text, 'middle');
  body += text(820, 358, '联系人用于持续对话，项目包含目录、用户消息、Plan 和运行设置。', 13, 400, C.secondary, 'middle');
  [['联系人', '打开无项目范围的会话', '●', C.purpleSoft, C.purple], ['项目', '进入项目四个工作面', '▱', C.blueSoft, C.blue], ['终端', '打开一个本地 PTY 会话', '>_', C.greenSoft, C.green], ['远端', '连接 SSH 或浏览 SFTP', '⇄', C.orangeSoft, C.orange]].forEach((item, i) => {
    const x = 466 + (i % 2) * 360, y = 414 + Math.floor(i / 2) * 132;
    body += card(x, y, 330, 104, `${icon(x + 18, y + 18, item[2], item[3], item[4], 38)}${text(x + 72, y + 39, item[0], 15, 680)}${text(x + 72, y + 64, item[1], 12, 400, C.secondary)}${text(x + 304, y + 58, '›', 22, 500, C.tertiary, 'middle')}`);
  });
  write('02-resource-shell.svg', resourceShell('ChatOS', '', body));
}

// 03 项目用户消息
{
  let body = rect(244, 150, 1196, 750, C.panel);
  body += text(270, 184, 'test_project · 叽咕狸', 13, 650, C.secondary);
  body += text(1410, 184, '执行中 · 可发送引导', 11, 600, C.purple, 'end') + line(244, 204, 1440, 204);

  body += circle(282, 246, 15, '#EEEFF2') + text(282, 251, '你', 10, 700, C.secondary, 'middle');
  body += text(310, 239, '你 · 09:31', 11, 650, C.secondary) + text(310, 269, '我想你再试试，我已经把刚才的问题解决了。', 14, 500);

  body += circle(282, 326, 15, C.purpleSoft) + text(282, 331, '叽', 10, 700, C.purple, 'middle');
  body += text(310, 319, '叽咕狸 · 09:31', 11, 650, C.secondary) + text(310, 349, '已重新发起整理与迁移。先验证 vault 可读，再按实际主题迁移。', 14, 500);
  body += card(310, 378, 1040, 108, `${pill(328, 396, '任务被阻塞', C.orangeSoft, C.orange, 86)}${text(328, 434, '重新整理并迁移 Obsidian 笔记到记事本', 13, 680)}${text(328, 459, '未修改源文件，也未写入猜测性的内容。', 12, 400, C.secondary)}${button(1038, 416, '查看过程', false, 84)}${button(1128, 416, '查看详情', false, 84)}${button(1218, 416, '任务图', true, 84)}`, '#FAFAFB', 11);

  body += circle(282, 544, 15, '#EEEFF2') + text(282, 549, '你', 10, 700, C.secondary, 'middle');
  body += text(310, 537, '你 · 10:53', 11, 650, C.secondary) + text(310, 567, '普通消息继续按时间线展示，不会生成任务入口。', 14, 500);
  body += circle(282, 624, 15, C.purpleSoft) + text(282, 629, '叽', 10, 700, C.purple, 'middle');
  body += text(310, 617, '叽咕狸 · 10:53', 11, 650, C.secondary) + text(310, 647, '收到，我会继续保留当前会话上下文。', 14, 500);

  body += card(270, 790, 1138, 82, `${pill(288, 806, 'my / gpt-5.6-terra', '#EEEFF2', C.secondary, 148)}${text(450, 823, '📎', 16, 500, C.secondary)}${pill(478, 806, '外挂程式', '#EEEFF2', C.secondary, 76)}${pill(562, 806, '规划 关', '#EEEFF2', C.secondary, 66)}${pill(636, 806, '推理 开', C.purpleSoft, C.purple, 66)}${text(288, 850, '执行中，可发送引导（不会打断当前执行）…', 12, 400, C.tertiary)}${circle(1378, 838, 17, C.blue)}${text(1378, 844, '↑', 15, 700, '#FFF', 'middle')}`, '#FCFCFD', 13);
  write('03-project-user-messages.svg', resourceShell('test_project', 'test_project', body, '用户消息') + visualPreview('Open Computer Use', '正在操作 Obsidian', 'open-computer-use'));
}

// 04 项目目录
{
  let body = rect(244, 150, 330, 750, '#F8F8FA') + line(574, 150, 574, 900) + rect(574, 150, 866, 750, C.panel);
  body += text(264, 180, '项目信息', 11, 600, C.secondary) + text(264, 205, 'test_project', 15, 680) + text(264, 226, '/test_project', 11, 400, C.secondary, 'start', mono);
  body += text(264, 254, '文件来自当前设备的 Local Connector 工作区。', 10, 400, C.secondary);
  body += button(264, 274, '选择根目录', false, 92) + button(362, 274, '新建目录', false, 78) + button(446, 274, '新建文件', false, 78);
  body += rect(264, 324, 290, 36, C.panel, 8, C.line) + text(280, 347, '⌕ 全文搜索注释、符号、字符串', 11, 400, C.tertiary) + text(478, 347, 'Aa  W', 11, 600, C.secondary);
  const tree = [['⌄ .chatos', 0], ['▸ .git', 0], ['▸ backend', 0], ['⌄ frontend', 0], ['▸ dist', 1], ['▸ node_modules', 1], ['▸ src', 1], ['  package.json  573 B', 1], ['  README.md  1.8 KB', 1], ['docker-compose.yml  2.02 KB', 0], ['README.md  16.98 KB', 0]];
  tree.forEach(([label, indent], i) => body += text(272 + indent * 18, 396 + i * 32, label, 11, label.includes('package.json') ? 650 : 500, label.includes('package.json') ? C.blue : C.text));
  body += text(596, 182, '文件预览', 13, 680) + text(596, 204, '/frontend/package.json', 11, 400, C.secondary, 'start', mono);
  body += button(1234, 170, '复制', false, 58) + button(1298, 170, '编辑', true, 58) + line(574, 222, 1440, 222);
  ['{', '  "name": "test-project-frontend",', '  "scripts": {', '    "dev": "vite",', '    "build": "tsc && vite build"', '  },', '  "dependencies": {', '    "react": "^19.0.0"', '  }', '}'].forEach((l, i) => body += text(616, 274 + i * 32, l, 13, 450, i === 0 || i === 9 ? C.secondary : C.text, 'start', mono));
  write('04-project-directory.svg', resourceShell('test_project', 'test_project', body, '项目目录', `${button(1320, 110, 'Git ⌄', false, 86)}`));
}

// 05 Plan / Requirement
{
  let body = rect(244, 150, 444, 750, '#F8F8FA') + line(688, 150, 688, 900) + rect(688, 150, 752, 750, C.panel);
  body += text(264, 180, 'Requirement', 14, 680) + pill(264, 198, '需求 8', '#EEEFF2', C.secondary, 62) + pill(332, 198, '完成 11', C.greenSoft, C.green, 70) + pill(408, 198, '阻塞 2', C.orangeSoft, C.orange, 64);
  body += line(258, 234, 674, 234) + text(264, 262, '项目需求', 11, 650, C.secondary) + text(434, 262, '›', 15, 500, C.tertiary) + text(456, 262, '客户端重写', 11, 650, C.secondary);
  const reqs = [['原生客户端重写', '进行中', 'P0', '前置 2', '完成客户端工作区与 Connector 原生化'], ['聊天历史可靠性', '进行中', 'P0', '后续 3', '稳定分页、实时合并和滚动锚点'], ['任务流程可视化', '待开始', 'P1', '前置 1', '保留 DAG、阻塞和 Run 详情'], ['插件运行时迁移', '待开始', 'P2', '后续 1', '安装、签名、OAuth 与回滚']];
  reqs.forEach((r, i) => {
    const y = 286 + i * 118;
    if (i === 1) body += rect(254, y - 8, 424, 106, C.blueSoft, 9, '#BFD8FF');
    body += text(270, y + 16, r[0], 13, i === 1 ? 680 : 600, i === 1 ? C.blue : C.text) + pill(270, y + 30, r[1], r[1] === '进行中' ? C.purpleSoft : '#EEEFF2', r[1] === '进行中' ? C.purple : C.secondary, 62) + pill(338, y + 30, r[2], C.orangeSoft, C.orange, 42) + pill(386, y + 30, r[3], '#EEEFF2', C.secondary, 58);
    body += text(270, y + 78, r[4], 11, 400, C.secondary);
  });
  body += pill(712, 174, '进行中', C.purpleSoft, C.purple, 64) + pill(782, 174, '功能需求', '#EEEFF2', C.secondary, 68) + pill(856, 174, 'P0', C.orangeSoft, C.orange, 42);
  body += text(712, 228, '聊天历史可靠性', 22, 720) + text(712, 252, '更新于 2026-08-24', 11, 400, C.secondary) + button(1196, 194, '预览流程', false, 88) + button(1292, 194, '打开执行工作台', true, 124) + line(712, 282, 1416, 282);
  ['需求', '技术文档 2', '任务 7'].forEach((tab, i) => {
    const x = 716 + i * 112;
    if (i === 0) body += line(x, 322, x + 70, 322, C.blue, 2);
    body += text(x + 35, 310, tab, 12, i === 0 ? 650 : 500, i === 0 ? C.text : C.secondary, 'middle');
  });
  body += card(712, 348, 704, 104, `${text(730, 374, '需求关系', 12, 680)}${text(730, 402, '前置需求', 11, 600, C.secondary)}${pill(806, 386, '会话 Store 基线', C.orangeSoft, C.orange, 100)}${text(730, 432, '执行会包含', 11, 600, C.secondary)}${pill(806, 416, '当前需求', C.blueSoft, C.blue, 72)}${pill(884, 416, '后续 3 个需求', '#EEEFF2', C.secondary, 96)}`, '#FAFAFB', 10);
  [['摘要', '重做稳定 Turn、分页、缓存、Realtime 合并和滚动恢复。'], ['详细说明', '切换会话不得清空；后台刷新只能 merge；历史页 prepend 保持锚点。'], ['业务价值', '长会话可以可靠继续，任务回调不会丢失或重复。'], ['验收标准', '离线重开、重连、加载旧页、运行中回调均通过专项测试。']].forEach((sec, i) => {
    const y = 490 + i * 88;
    body += text(712, y, sec[0], 12, 680) + text(712, y + 26, sec[1], 12, 450, C.secondary);
  });
  write('05-project-plan.svg', resourceShell('test_project', 'test_project', body, 'Plan'));
}

// 06 Requirement 执行工作台
{
  let body = rect(244, 102, 1196, 798, C.panel);
  body += text(270, 136, '执行计划工作台', 20, 720) + pill(466, 116, '云端编排 / Local Connector 承载', C.purpleSoft, C.purple, 196);
  body += text(270, 164, '聊天历史可靠性', 12, 500, C.secondary) + button(1174, 116, '↻ 刷新', false, 78) + button(1260, 116, '全屏', false, 70) + button(1338, 116, '关闭', false, 70) + line(244, 184, 1440, 184);

  // Real source layout: planning process sidebar + live task dependency graph.
  body += rect(244, 184, 350, 716, '#F8F8FA') + line(594, 184, 594, 900);
  body += circle(270, 220, 7, C.orange) + text(290, 224, '流程图已就绪，等待确认执行', 14, 680) + text(270, 250, '任务尚未启动，可以继续调整本次执行计划。', 11, 450, C.secondary);
  body += line(264, 274, 574, 274) + text(270, 306, '规划过程', 12, 680) + button(448, 288, '详细过程 12', false, 116);
  const planningSteps = [
    ['✓', '读取 Requirement、任务与技术文档', C.green],
    ['✓', '生成 7 个执行任务', C.green],
    ['✓', '构建 6 条任务依赖', C.green],
    ['●', '等待用户确认执行', C.orange],
  ];
  planningSteps.forEach((step, i) => {
    const y = 354 + i * 62;
    if (i < planningSteps.length - 1) body += line(280, y + 12, 280, y + 54, C.line);
    body += circle(280, y, 10, C.panel, step[2]) + text(280, y + 4, step[0], 9, 700, step[2], 'middle') + text(300, y + 4, step[1], 11, 550);
  });
  body += line(264, 580, 574, 580) + text(270, 612, '调整执行计划', 12, 680);
  body += rect(270, 628, 298, 104, C.panel, 8, C.line) + text(286, 654, '例如：先补测试，再修改接口；', 11, 400, C.tertiary) + text(286, 676, '把前端和后端拆开执行…', 11, 400, C.tertiary);
  body += text(270, 758, '发送后会生成新的任务依赖图，', 10, 450, C.secondary) + text(270, 776, '不会直接启动任务。', 10, 450, C.secondary) + button(438, 750, '发送并调整', true, 130);

  // The graph is the primary execution-plan surface, matching RequirementExecutionGraphSurface.
  body += rect(594, 184, 846, 716, C.panel);
  body += text(620, 220, '⌘  实时执行流程图', 14, 680) + text(620, 244, '任务节点和依赖关系会随规划结果实时更新。', 11, 450, C.secondary);
  body += pill(1174, 204, '节点 7', '#EEEFF2', C.secondary, 66) + pill(1246, 204, '依赖 6', '#EEEFF2', C.secondary, 66) + pill(1318, 204, '运行记录 0', '#EEEFF2', C.secondary, 92) + line(594, 264, 1440, 264);
  body += rect(614, 284, 806, 500, '#FAFAFB', 10, C.line);
  body += pill(632, 300, '等待执行', C.orangeSoft, C.orange, 72) + text(716, 317, '点击节点查看任务详情、过程与 Run', 10, 450, C.secondary);
  body += button(1212, 298, '精简图', true, 70) + button(1288, 298, '完整图', false, 70) + button(1364, 298, '适应', false, 52);

  body += pathEl('M824 458 H870 V374 H900', C.blue, 2) + pathEl('M824 458 H870 V554 H900', C.blue, 2);
  body += pathEl('M1090 374 H1120 V458 H1144', C.blue, 2) + pathEl('M1090 554 H1120 V458', C.blue, 2) + pathEl('M1239 502 V584', C.blue, 2);
  body += card(638, 414, 186, 88, `${pill(652, 426, '任务 1', C.blueSoft, C.blue, 56)}${text(652, 466, '建立历史 Store', 12, 680)}${text(652, 488, '无前置 · P0', 10, 450, C.secondary)}`, C.panel, 10, C.blue);
  body += card(900, 330, 190, 88, `${pill(914, 342, '任务 2', '#EEEFF2', C.secondary, 56)}${text(914, 382, 'SQLite 缓存恢复', 12, 680)}${text(914, 404, '前置：任务 1', 10, 450, C.secondary)}`, C.panel, 10);
  body += card(900, 510, 190, 88, `${pill(914, 522, '任务 3', '#EEEFF2', C.secondary, 56)}${text(914, 562, 'Realtime 合并去重', 12, 680)}${text(914, 584, '前置：任务 1', 10, 450, C.secondary)}`, C.panel, 10);
  body += card(1144, 414, 190, 88, `${pill(1158, 426, '任务 4', '#EEEFF2', C.secondary, 56)}${text(1158, 466, '分页与滚动恢复', 12, 680)}${text(1158, 488, '前置：任务 2、3', 10, 450, C.secondary)}`, C.panel, 10);
  body += card(1144, 584, 190, 88, `${pill(1158, 596, '任务 5–7', '#EEEFF2', C.secondary, 66)}${text(1158, 636, '压力测试与发布门禁', 12, 680)}${text(1158, 658, '前置：任务 4', 10, 450, C.secondary)}`, C.panel, 10);

  body += line(594, 804, 1440, 804) + text(620, 840, '流程图已就绪，当前还没有任务开始运行。', 11, 450, C.secondary);
  body += button(1250, 822, '关闭', false, 72) + button(1330, 822, '执行', true, 78);
  write('06-requirement-execution.svg', resourceShell('Requirement 执行', 'test_project', body));
}

// 07 项目运行设置
{
  let body = rect(244, 150, 1196, 750, '#FAFAFB');
  body += text(270, 184, 'test_project', 17, 700) + text(270, 207, '/test_project', 11, 400, C.secondary, 'start', mono);
  body += pill(270, 226, '运行状态：就绪', C.greenSoft, C.green, 104) + pill(382, 226, '检测目标 3', '#EEEFF2', C.secondary, 76) + pill(466, 226, 'Java', '#EEEFF2', C.secondary, 50);
  body += card(270, 278, 540, 146, `${text(290, 306, '运行前检查', 12, 680)}${text(290, 340, '✓ 当前目标没有阻塞问题', 13, 600, C.green)}${text(290, 370, '其他目标有 1 个环境提示', 11, 400, C.secondary)}${button(664, 326, '查看其他目标', false, 116)}`);
  body += card(830, 278, 580, 146, `${text(850, 306, '运行目标', 12, 680)}${rect(850, 324, 350, 36, C.panel, 8, C.line)}${text(866, 347, 'Spring Boot · Maven spring-boot:run', 12, 500)}${text(850, 386, 'backend · pom.xml · mvn spring-boot:run', 11, 400, C.secondary, 'start', mono)}${button(1216, 324, '启动新实例', true, 112)}`);
  body += card(270, 446, 1140, 190, `${text(290, 474, '运行实例', 12, 680)}${pill(1182, 458, '2 个实例', '#EEEFF2', C.secondary, 76)}${card(290, 494, 250, 96, `${text(310, 522, '实例 1', 13, 650)}${pill(430, 506, '运行中', C.greenSoft, C.green, 62)}${text(310, 550, 'terminal_… · process active', 11, 400, C.secondary)}${text(310, 574, '最近输出 3 秒前', 10, 400, C.tertiary)}`, C.blueSoft, 9, '#BFD8FF')}${card(554, 494, 250, 96, `${text(574, 522, '实例 2', 13, 650)}${pill(694, 506, '已停止', '#EEEFF2', C.secondary, 62)}${text(574, 550, 'exit 1 · 配置缺失', 11, 400, C.secondary)}`, C.panel, 9)}${button(920, 510, '停止当前', false, 88)}${button(1016, 510, '重启', false, 70)}${button(1094, 510, '删除', false, 70)}${button(1172, 510, '刷新', false, 70)}`);
  body += card(270, 658, 1140, 206, `${text(290, 686, '运行终端 · 实例 1', 12, 680)}${rect(290, 706, 1100, 132, C.dark, 8)}${text(310, 734, '[INFO] Starting application…', 12, 450, '#C9CED7', 'start', mono)}${text(310, 762, '[INFO] Listening on 127.0.0.1:8080', 12, 450, '#7DD3A6', 'start', mono)}${text(310, 790, '[INFO] Application started', 12, 450, '#7DD3A6', 'start', mono)}${text(310, 822, '工具链与环境变量  ▸', 11, 600, '#AEB5C0')}`);
  write('07-project-run-settings.svg', resourceShell('test_project', 'test_project', body, '项目设置'));
}

// 08 Connector 设备配对
{
  let body = pageIntro(270, 'CONNECTION', '设备配对', '查看本机设备、云端连接与安全边界。');
  body += card(270, 250, 550, 250, `${text(294, 282, '连接状态', 16, 700)}${circle(298, 318, 7, C.green)}${text(318, 323, '已连接', 13, 650, C.green)}${text(294, 356, 'Core 地址', 11, 600, C.secondary)}${text(430, 356, 'http://127.0.0.1:39230', 12, 500, C.text, 'start', mono)}${text(294, 390, '用户', 11, 600, C.secondary)}${text(430, 390, '设计账号', 12, 500)}${text(294, 424, '设备', 11, 600, C.secondary)}${text(430, 424, 'Local Connector', 12, 500)}${text(294, 458, 'Device ID', 11, 600, C.secondary)}${text(430, 458, 'c27f…1251', 12, 500, C.text, 'start', mono)}${button(662, 444, '退出本机配对', false, 126)}`);
  body += card(844, 250, 566, 250, `${text(868, 282, '本机边界', 16, 700)}${text(868, 318, '文件、终端和本机权限控制运行在当前电脑。', 13, 500)}${text(868, 344, '云端只通过已登录设备的长连接发起授权请求。', 12, 400, C.secondary)}${line(868, 372, 1384, 372)}${text(868, 404, '文件路由', 11, 600, C.secondary)}${text(1368, 404, '自动连接本机', 12, 600, C.green, 'end')}${text(868, 438, '权限控制', 11, 600, C.secondary)}${text(1368, 438, '设置页管理', 12, 600, C.blue, 'end')}${text(868, 472, '运行方式', 11, 600, C.secondary)}${text(1368, 472, '本机进程', 12, 600, C.text, 'end')}`);
  body += card(270, 530, 1140, 160, `${text(294, 562, '关键说明', 14, 680)}${text(294, 598, 'Local Connector 默认连接本机文件系统，无需另行登记目录。', 13, 600)}${text(294, 626, '任务文件权限、沙箱策略和 macOS 系统权限仍然独立生效。', 12, 400, C.secondary)}${pill(1164, 554, '本机安全边界', C.greenSoft, C.green, 112)}`, '#F8F8FA', 12);
  write('08-connector-device-pairing.svg', connectorShell('设备配对', body));
}

// 09 Plugin Marketplace
{
  let body = pageIntro(270, 'PLUGIN MARKETPLACE', '外挂程式', '浏览可信 Catalog，检查 Release、组件、权限、更新与回滚。');
  body += pill(270, 244, 'Catalog 2', '#EEEFF2', C.secondary, 78) + pill(354, 244, '已安装 2', C.greenSoft, C.green, 82) + pill(442, 244, '可更新 0', '#EEEFF2', C.secondary, 82);
  body += button(1130, 240, '检查更新', false, 92) + button(1228, 240, '恢复事务', false, 92) + button(1326, 240, '刷新', true, 72);
  body += card(270, 292, 1140, 92, `${text(294, 320, 'ON THIS DEVICE', 10, 700, C.tertiary)}${pill(294, 338, 'Open Computer Use · v0.3.38', C.blueSoft, C.blue, 190)}${pill(492, 338, 'Browser CDP · v0.1.0', C.blueSoft, C.blue, 164)}${text(1378, 356, '已签名', 11, 600, C.green, 'end')}`);
  body += rect(270, 406, 520, 36, C.panel, 8, C.line) + text(290, 429, '⌕ 搜索 Plugin、publisher 或分类', 11, 400, C.tertiary) + pill(808, 412, '公开', C.blueSoft, C.blue, 54) + pill(868, 412, '个人', '#EEEFF2', C.secondary, 54) + rect(940, 406, 170, 36, C.panel, 8, C.line) + text(956, 429, '全部分类 ⌄', 11, 500, C.secondary);
  [['Open Computer Use', 'OpenAI', 'Control local desktop applications through MCP.', 'v0.3.38'], ['Browser CDP', 'ChatOS', 'Operate managed Chromium or connected Chrome.', 'v0.1.0']].forEach((p, i) => {
    const y = 472 + i * 154;
    body += card(270, y, 1140, 132, `${icon(292, y + 20, i === 0 ? '⌁' : '◎', i === 0 ? C.purpleSoft : C.blueSoft, i === 0 ? C.purple : C.blue, 42)}${text(350, y + 36, p[0], 15, 700)}${text(350, y + 58, p[1], 11, 500, C.secondary)}${text(292, y + 88, p[2], 12, 400, C.secondary)}${pill(292, y + 98, p[3], '#EEEFF2', C.secondary, 72)}${pill(370, y + 98, 'Skills', '#EEEFF2', C.secondary, 58)}${pill(434, y + 98, '自动更新', C.greenSoft, C.green, 72)}${button(1118, y + 50, '详情', false, 70)}${button(1196, y + 50, i === 0 ? '回滚' : '权限', false, 70)}${button(1274, y + 50, '卸载', false, 70)}`);
  });
  write('09-connector-plugins.svg', connectorShell('外挂程式', body));
}

// 10 命令审批
{
  let body = pageIntro(270, 'APPROVAL', '命令审批', '控制敏感命令与本机操作的审批级别、白名单和历史。');
  body += card(270, 246, 1140, 112, `${text(294, 278, '审批模式', 13, 680)}${pill(294, 298, '请求审批', '#EEEFF2', C.secondary, 86)}${pill(386, 298, 'AI 自动审批', '#EEEFF2', C.secondary, 96)}${pill(488, 298, '从不询问', C.redSoft, C.red, 86)}${text(600, 316, '当前：命令直接执行，不再弹出审批', 12, 600, C.red)}${button(1308, 290, '刷新', false, 72)}`);
  body += card(270, 382, 1140, 106, `${text(294, 414, '待审批', 13, 680)}${circle(300, 450, 6, C.green)}${text(320, 454, '当前没有待审批命令或操作', 12, 500, C.secondary)}${pill(1250, 424, '0 项', C.greenSoft, C.green, 62)}`);
  body += card(270, 512, 1140, 150, `${text(294, 544, '白名单', 13, 680)}${text(294, 570, '209 条始终允许命令 · 4 个项目', 11, 400, C.secondary)}${pill(294, 590, 'test_project · 203', C.blueSoft, C.blue, 128)}${pill(428, 590, 'workspace · 1', '#EEEFF2', C.secondary, 100)}${pill(534, 590, '其他项目 · 5', '#EEEFF2', C.secondary, 96)}${text(1190, 612, '第 1 / 21 页', 11, 500, C.secondary)}${button(1282, 594, '下一页', false, 84)}`);
  body += card(270, 686, 1140, 174, `${text(294, 718, '审批历史', 13, 680)}${text(1260, 718, '688 条记录', 11, 500, C.secondary)}${pill(294, 742, '通过', C.greenSoft, C.green, 52)}${pill(352, 742, '高风险', C.redSoft, C.red, 58)}${text(424, 758, 'docker compose up -d --no-deps frontend', 11, 500, C.text, 'start', mono)}${text(424, 784, 'test_project · 从不询问 · Task Runner · 09:27', 10, 400, C.secondary)}${line(294, 806, 1386, 806)}${pill(294, 822, '拒绝', C.redSoft, C.red, 52)}${pill(352, 822, '低风险', '#EEEFF2', C.secondary, 58)}${text(424, 838, 'plugin-mcp-tool-call · browser_session_open', 11, 500, C.text, 'start', mono)}`);
  write('10-connector-command-approval.svg', connectorShell('命令审批', body));
}

// 11 智能体管理
{
  let body = rect(244, 102, 1196, 798, '#FAFAFB');
  body += text(270, 144, '智能体管理', 22, 720) + text(270, 170, 'Agent 配置与联系人是不同实体；联系人只是会话入口。', 12, 400, C.secondary);
  body += button(1162, 126, 'AI 创建', false, 88) + button(1258, 126, '新建智能体', true, 112) + line(270, 190, 1410, 190);
  body += card(270, 218, 1140, 150, `${icon(292, 240, '叽', C.purpleSoft, C.purple, 44)}${text(352, 252, '叽咕狸', 17, 700)}${pill(352, 270, '启用', C.greenSoft, C.green, 50)}${pill(408, 270, 'assistant', '#EEEFF2', C.secondary, 70)}${text(292, 316, '新用户默认助手，帮助整理需求、持续对话和使用 Task Runner。', 12, 400, C.secondary)}${pill(292, 332, '插件 0', '#EEEFF2', C.secondary, 58)}${pill(356, 332, '技能 0', '#EEEFF2', C.secondary, 58)}${button(1210, 260, '编辑', false, 72)}${button(1290, 260, '删除', false, 72)}`);
  body += card(270, 398, 1140, 420, `${text(294, 430, '新建智能体', 16, 700)}${text(294, 468, '名称', 12, 600)}${rect(294, 482, 500, 38, C.panel, 8, C.line)}${text(310, 506, '输入智能体名称', 11, 400, C.tertiary)}${text(822, 468, '分类', 12, 600)}${rect(822, 482, 500, 38, C.panel, 8, C.line)}${text(838, 506, '输入分类', 11, 400, C.tertiary)}${text(294, 556, '描述', 12, 600)}${rect(294, 570, 1028, 44, C.panel, 8, C.line)}${text(310, 597, '补充用途和边界', 11, 400, C.tertiary)}${text(294, 650, '角色定义', 12, 600)}${rect(294, 664, 1028, 96, C.panel, 8, C.line)}${text(310, 690, '描述职责、行为边界和输出风格…', 11, 400, C.tertiary)}${pill(294, 778, '✓ 启用', C.greenSoft, C.green, 68)}${button(1136, 772, '取消', false, 80)}${button(1224, 772, '创建智能体', true, 98)}`);
  write('11-agent-management.svg', resourceShell('智能体管理', '', body));
}

// 12 创建项目：单工作区、单目录
{
  let s = chrome('ChatOS');
  s += rect(0, 48, 1440, 852, '#F4F4F7') + card(380, 94, 680, 714, '', C.panel, 18);
  s += text(416, 140, '创建项目', 24, 720) + text(416, 168, '从一个 Local Connector 工作区选择一个目录。', 12, 400, C.secondary) + line(416, 190, 1024, 190);
  s += text(416, 226, 'Local Connector 工作区', 12, 600) + rect(416, 240, 520, 42, C.panel, 9, C.line) + text(432, 266, '本机文件系统 · Local Connector', 13, 500) + text(914, 266, '⌄', 14, 600, C.secondary) + button(944, 244, '刷新', false, 70);
  s += text(416, 318, '项目名称将使用', 11, 500, C.secondary) + text(540, 318, 'chatos_swift', 12, 650) + text(416, 344, '当前目录', 11, 500, C.secondary) + text(496, 344, '本机文件系统/Volumes/MacPortable', 11, 500, C.text, 'start', mono);
  s += button(416, 362, '上一级', false, 72) + button(494, 362, '选择当前目录', true, 112);
  ['project', 'chatos_swift', 'chatos_rs', 'Documents', 'Downloads'].forEach((d, i) => {
    const y = 414 + i * 48;
    if (i === 1) s += rect(416, y, 608, 40, C.blueSoft, 8);
    s += text(436, y + 25, '▱', 13, 600, i === 1 ? C.blue : C.secondary) + text(464, y + 25, d, 12, i === 1 ? 650 : 500, i === 1 ? C.blue : C.text) + text(998, y + 25, '打开', 11, 500, C.secondary, 'end');
  });
  s += text(416, 682, '在当前目录中新建文件夹', 11, 600, C.secondary) + rect(416, 696, 460, 38, C.panel, 8, C.line) + text(432, 720, '新目录名称', 11, 400, C.tertiary) + button(886, 699, '新建目录', false, 98);
  s += button(824, 756, '取消', false, 88) + button(920, 756, '创建', true, 104);
  write('12-project-creation.svg', s);
}

// 13 正常会话中的聊天历史行为
{
  let body = rect(244, 102, 1196, 798, C.panel);
  body += text(270, 139, 'test_project · 叽咕狸', 16, 700) + text(1398, 139, '任务执行中 · 可发送引导', 11, 600, C.purple, 'end') + line(270, 160, 1410, 160);
  body += button(742, 178, '↑ 加载更早消息', false, 136) + line(300, 232, 1380, 232) + pill(772, 220, '2026 年 8 月 22 日', C.panel, C.tertiary, 132);
  body += circle(300, 286, 17, '#E6E8ED') + text(300, 292, '你', 11, 650, C.secondary, 'middle') + rect(328, 260, 790, 62, '#F3F4F6', 12) + text(348, 286, '继续检查聊天历史，切换会话后也要回到我刚才看到的位置。', 13, 500) + text(1090, 308, '10:42', 10, 400, C.tertiary, 'end');
  body += circle(300, 372, 17, C.purpleSoft) + text(300, 378, '叽', 11, 700, C.purple, 'middle') + text(328, 362, '叽咕狸 · 10:42', 11, 650, C.secondary) + text(328, 394, '我会先验证分页、会话切换和新消息到达时的滚动行为。', 13, 500);
  body += card(328, 424, 790, 96, `${pill(346, 442, '正在执行', C.purpleSoft, C.purple, 72)}${text(346, 482, '验证长会话历史与滚动恢复', 13, 680)}${text(346, 504, '已完成 3 / 5 · 正在测试会话切换', 11, 500, C.secondary)}${button(1012, 456, '查看过程', false, 82)}`, '#FAFAFB', 10);
  body += circle(300, 566, 17, C.purpleSoft) + text(300, 572, '叽', 11, 700, C.purple, 'middle') + text(328, 556, '叽咕狸 · 刚刚', 11, 650, C.secondary) + text(328, 588, '加载更早消息后位置保持不变。接下来验证有新回复时不会把你拉回底部。', 13, 500);
  body += line(300, 636, 1380, 636) + rect(696, 650, 256, 38, C.blue, 19) + text(824, 675, '↓ 3 条新消息', 12, 650, '#FFF', 'middle');
  body += card(270, 724, 1140, 148, `${pill(290, 742, 'my / gpt-5.6-terra', '#EEEFF2', C.secondary, 148)}${text(454, 759, '📎', 16, 500, C.secondary)}${pill(484, 742, '外挂程式', '#EEEFF2', C.secondary, 76)}${pill(568, 742, '规划 关', '#EEEFF2', C.secondary, 66)}${pill(642, 742, '推理 开', C.purpleSoft, C.purple, 66)}${text(290, 806, '输入消息或在任务执行中发送引导…', 13, 400, C.tertiary)}${circle(1374, 814, 18, C.blue)}${text(1374, 820, '↑', 15, 700, '#FFF', 'middle')}${line(290, 834, 1350, 834)}${text(290, 856, '执行中的引导不会清空当前历史', 10, 450, C.secondary)}`, '#FCFCFD', 13);
  write('13-chat-history.svg', resourceShell('聊天', 'test_project', body));
}

// 14 任务流程图
{
  let body = rect(244, 102, 1196, 798, '#F8F8FA');
  body += pill(270, 120, '当前任务', C.blueSoft, C.blue, 72) + pill(348, 120, '直接前置', C.orangeSoft, C.orange, 78) + pill(432, 120, '间接前置', '#EEEFF2', C.secondary, 78) + line(526, 132, 560, 132, C.tertiary, 1.5, '6 5') + text(570, 136, 'Context（不阻塞）', 11, 500, C.secondary);
  body += button(1120, 116, '精简图', true, 74) + button(1200, 116, '完整图', false, 74) + button(1280, 116, '适应窗口', false, 96) + line(270, 158, 1410, 158);
  body += pathEl('M820 286 V328 H560 V364', C.green, 2.2) + pathEl('M820 286 V328 H1000 V364', C.purple, 2.2) + pathEl('M560 472 V520 H820 V556', C.orange, 2.2) + pathEl('M1000 472 V520 H820', C.blue, 2.2) + pathEl('M820 664 V716', C.blue, 2.2) + pathEl('M678 420 H704 V210 H1124 V420 H1148', C.tertiary, 1.5, 'none', '7 7');
  const nodes = [[700, 206, 240, 80, '冻结协议与安全边界', '已完成', C.green, C.greenSoft, '间接前置'], [440, 364, 240, 108, '聊天历史 Store', '已完成', C.orange, C.orangeSoft, '直接前置'], [880, 364, 240, 108, '任务图布局引擎', '运行中', C.purple, C.purpleSoft, '当前任务'], [700, 556, 240, 108, 'SwiftUI Task Graph', '等待前置', C.blue, C.blueSoft, '后续任务'], [700, 716, 240, 82, '集成测试与发布', '待开始', C.secondary, '#EEEFF2', '后续任务']];
  nodes.forEach((n, i) => {
    const [x, y, w, h, titleValue, status, color, soft, relation] = n;
    body += card(x, y, w, h, `${circle(x + 18, y + 20, 6, color)}${text(x + 32, y + 24, relation, 10, 600, color)}${text(x + 16, y + 52, titleValue, 13, 680)}${pill(x + 16, y + h - 32, status, soft, color, status === '等待前置' ? 74 : 64)}${text(x + w - 16, y + h - 15, i < 2 ? '2 / 2' : i === 2 ? '3 / 5' : '0 / 4', 10, 500, C.secondary, 'end')}`, C.panel, 12, i === 2 ? C.purple : C.line);
  });
  body += card(1160, 184, 250, 612, `${text(1180, 214, '任务检查器', 15, 700)}${pill(1180, 232, '运行中', C.purpleSoft, C.purple, 64)}${text(1180, 282, '任务图布局引擎', 15, 680)}${text(1180, 310, '上游 2 · 下游 2', 11, 500, C.secondary)}${text(1180, 352, '可用动作', 11, 650, C.secondary)}${button(1180, 370, '查看执行过程', true, 206)}${button(1180, 410, '处理阻塞', false, 206)}${button(1180, 450, 'Run 详情', false, 206)}${text(1180, 512, '最近 Run', 11, 650, C.secondary)}${text(1180, 542, 'run_8F3A', 12, 650, C.text, 'start', mono)}${text(1180, 568, '8 / 12 tests · 02:16', 11, 400, C.secondary)}${text(1180, 624, '焦点行为', 11, 650, C.secondary)}${text(1180, 652, '保留完整上下游', 11, 500)}${text(1180, 676, '无关节点弱化', 11, 500)}${button(1180, 728, '清除聚焦', false, 206)}`);
  write('14-task-flow-graph.svg', resourceShell('任务流程图', 'test_project', body));
}

// 15 记事本
{
  let body = rect(160, 92, 1120, 720, C.window, 16, C.line) + rect(160, 92, 1120, 52, '#F7F7F8', 16) + rect(160, 128, 1120, 16, '#F7F7F8') + line(160, 144, 1280, 144);
  body += circle(184, 118, 6, '#FF5F57') + circle(204, 118, 6, '#FEBB2E') + circle(224, 118, 6, '#28C840') + text(258, 123, '记事本', 13, 680);
  body += text(1000, 123, '编辑', 11, 650, C.blue) + text(1052, 123, '预览', 11, 550, C.secondary) + text(1104, 123, '分栏', 11, 550, C.secondary) + text(1170, 123, '···', 16, 650, C.secondary);
  body += rect(160, 144, 270, 668, '#F3F3F5') + line(430, 144, 430, 812) + rect(430, 144, 850, 668, '#FFF');
  body += rect(180, 166, 230, 34, '#FFF', 8, C.line) + text(194, 188, '⌕ 搜索笔记', 11, 450, C.tertiary) + text(388, 188, '＋', 17, 550, C.blue, 'middle');
  body += text(182, 232, '文件夹', 10, 700, C.tertiary);
  [['▾ 飞书', false], ['  飞书消息摘要', false], ['▾ 项目', false], ['  EWO 项目信息摘要', true], ['▸ 每日摘要', false], ['▸ 归档', false]].forEach((item, i) => {
    const y = 258 + i * 42;
    if (item[1]) body += rect(174, y - 22, 242, 34, '#DDEBFF', 8);
    body += text(190, y, item[0], 12, item[1] ? 650 : 500, item[1] ? C.blue : C.text);
  });
  body += text(478, 206, 'EWO 项目信息摘要', 27, 720) + text(478, 234, '飞书 · 项目 · 摘要', 11, 500, C.secondary) + text(1228, 222, '已保存', 10, 600, C.green, 'end');
  body += line(478, 258, 1236, 258) + text(478, 310, '项目目标', 17, 700) + text(478, 344, '整理当前项目资料、关键决策和后续计划，让团队可以持续接手。', 13, 450, C.text);
  body += text(478, 410, '待办', 17, 700) + circle(484, 446, 6, '#FFF', C.line) + text(502, 450, '核对需求范围', 13, 450) + circle(484, 480, 6, '#FFF', C.line) + text(502, 484, '补充技术文档', 13, 450);
  body += text(478, 552, '关键决策', 17, 700) + text(478, 586, '客户端主工作区使用 SwiftUI 原生实现，浏览器 Web 端保持独立。', 13, 450) + text(478, 620, '聊天历史使用稳定 Turn 与 merge-only 更新。', 13, 450);
  body += line(478, 742, 1236, 742) + text(478, 772, '最后编辑于今天 10:24', 10, 450, C.tertiary) + text(1236, 772, '782 字', 10, 450, C.tertiary, 'end');
  write('15-notepad.svg', body);
}

// 16 远端连接
{
  let body = rect(244, 102, 1196, 798, '#F5F5F7') + card(398, 126, 888, 738, '', C.panel, 16);
  body += text(430, 166, '新增远端连接', 22, 720) + text(430, 192, 'SSH、SFTP 和凭据文件由所选 Local Connector 设备执行和读取。', 12, 400, C.secondary) + line(430, 214, 1254, 214);
  [['名称（可选）', '默认：user@host'], ['主机', '例如 1.2.3.4'], ['端口', '22'], ['用户名', 'root']].forEach((f, i) => {
    const col = i % 2, row = Math.floor(i / 2), x = 430 + col * 410, y = 248 + row * 86;
    body += text(x, y, f[0], 11, 600, C.secondary) + rect(x, y + 14, 382, 38, C.panel, 8, C.line) + text(x + 14, y + 38, f[1], 11, 400, C.tertiary);
  });
  body += text(430, 432, '主机校验策略', 11, 600, C.secondary) + rect(430, 446, 382, 38, C.panel, 8, C.line) + text(444, 470, '严格校验 ⌄', 11, 500);
  body += text(840, 432, '执行位置', 11, 600, C.secondary) + rect(840, 446, 382, 38, C.panel, 8, C.line) + text(854, 470, '本机文件系统 · Local Connector ⌄', 11, 500);
  body += text(430, 520, '认证方式', 11, 600, C.secondary) + rect(430, 534, 382, 38, C.panel, 8, C.line) + text(444, 558, '私钥 ⌄', 11, 500);
  body += text(840, 520, '默认远端目录（可选）', 11, 600, C.secondary) + rect(840, 534, 382, 38, C.panel, 8, C.line) + text(854, 558, '/home/root', 11, 400, C.tertiary);
  body += text(430, 606, '私钥路径', 11, 600, C.secondary) + rect(430, 620, 660, 38, C.panel, 8, C.line) + text(444, 644, '/Users/you/.ssh/id_rsa', 11, 400, C.tertiary, 'start', mono) + button(1100, 623, '选择文件', false, 122);
  body += pill(430, 688, '□ 启用跳板机', '#EEEFF2', C.secondary, 112) + text(430, 728, '启用后可复用已有远端连接，或填写独立 jump host 凭据。', 11, 400, C.secondary);
  body += button(946, 794, '取消', false, 86) + button(1040, 794, '测试连接', false, 96) + button(1144, 794, '创建', true, 78);
  write('16-remote-connection.svg', resourceShell('远端', '', body));
}

// 17 Connector 模型配置
{
  let body = pageIntro(270, 'MODELS', '模型配置', '同步云端模型，并选择本机命令审批使用的模型。');
  body += card(270, 248, 1140, 250, `${text(294, 280, '云端模型', 15, 700)}${text(294, 306, '供应商、凭据与运行参数由云端管理；客户端同步只读副本。', 12, 400, C.secondary)}${button(1268, 270, '同步云端模型', false, 116)}${line(294, 330, 1386, 330)}${pill(294, 354, '可用模型 7', '#EEEFF2', C.secondary, 88)}${text(294, 402, 'my / gpt-5.6-terra', 12, 650)}${pill(520, 386, '启用', C.greenSoft, C.green, 50)}${pill(576, 386, '凭据已同步', C.blueSoft, C.blue, 88)}${text(294, 446, 'my / gpt-5.6-luna', 12, 650)}${pill(520, 430, '启用', C.greenSoft, C.green, 50)}${pill(576, 430, '凭据已同步', C.blueSoft, C.blue, 88)}${text(900, 402, 'my / gpt-5.4', 12, 650)}${pill(1080, 386, '停用', '#EEEFF2', C.secondary, 50)}${pill(1136, 386, '凭据已同步', C.blueSoft, C.blue, 88)}`);
  body += card(270, 526, 1140, 280, `${text(294, 558, '本机审批 Agent 设置', 15, 700)}${text(294, 584, '本机仅配置审批模型及其审批运行参数。', 12, 400, C.secondary)}${text(294, 628, '模型请求最大重试次数', 11, 600, C.secondary)}${rect(294, 642, 200, 38, C.panel, 8, C.line)}${text(394, 666, '5', 12, 600, C.text, 'middle')}${text(294, 716, '命令审批模型', 11, 600, C.secondary)}${rect(294, 730, 440, 38, C.panel, 8, C.line)}${text(310, 754, 'my / gpt-5.6-luna ⌄', 11, 500)}${text(772, 716, '审批 Thinking', 11, 600, C.secondary)}${rect(772, 730, 280, 38, C.panel, 8, C.line)}${text(788, 754, 'minimal ⌄', 11, 500)}${button(1240, 720, '保存审批设置', true, 130)}`);
  write('17-connector-models.svg', connectorShell('模型配置', body));
}

// 18 Connector 运行与系统权限
{
  let body = pageIntro(270, 'RUNTIME & PERMISSIONS', '运行与权限', '调整本机运行参数，并检查 Skills 与 MCP 所需的系统权限。');
  body += card(270, 248, 1140, 132, `${text(294, 280, '运行配置', 14, 700)}${pill(294, 298, '开发者模式', C.orangeSoft, C.orange, 84)}${text(390, 316, 'ChatOS 127.0.0.1:8088 · Connector 39230 · User Service 39190', 11, 500, C.secondary, 'start', mono)}${text(294, 350, '切换环境会主动断开当前 Connector 长连接，避免本地页面与线上 Relay 混用。', 11, 400, C.secondary)}${button(1260, 292, '保存配置', true, 104)}`);
  body += card(270, 402, 1140, 112, `${text(294, 434, '本机命令审批 Agent', 14, 700)}${text(294, 462, '当前版本 17 · 最新版本 17 · Prompt 4/4 · 审批策略 1/1', 11, 500, C.secondary)}${pill(294, 478, '已安装', C.greenSoft, C.green, 64)}${button(1180, 438, '检查更新', false, 88)}${button(1276, 438, '更新', false, 88)}`);
  body += card(270, 536, 1140, 304, `${text(294, 568, 'Skills 与 MCP 系统权限', 14, 700)}${text(294, 596, '本地目录读写', 12, 650)}${pill(520, 580, '已就绪', C.greenSoft, C.green, 64)}${text(620, 596, '任务权限与 macOS 权限仍独立生效', 11, 400, C.secondary)}${button(1228, 580, '完全磁盘访问', false, 130)}${line(294, 620, 1386, 620)}${text(294, 652, '本机终端执行', 12, 650)}${pill(520, 636, '已就绪', C.greenSoft, C.green, 64)}${button(1228, 636, '开发者工具权限', false, 130)}${line(294, 676, 1386, 676)}${text(294, 708, '辅助功能控制', 12, 650)}${pill(520, 692, '由插件授权', C.orangeSoft, C.orange, 84)}${text(620, 708, 'Open Computer Use v0.3.38', 11, 400, C.secondary)}${button(1228, 692, '启动权限引导', false, 130)}${line(294, 732, 1386, 732)}${text(294, 764, '屏幕录制', 12, 650)}${pill(520, 748, '由插件授权', C.orangeSoft, C.orange, 84)}${text(620, 764, 'Connector 自身授权不代表插件已授权', 11, 400, C.secondary)}${button(1228, 748, '启动权限引导', false, 130)}${text(294, 818, 'Office 自动化按目标应用单独授权。', 11, 400, C.secondary)}`);
  write('18-connector-runtime-permissions.svg', connectorShell('运行与权限', body));
}

// 19 Connector 权限控制
{
  let body = pageIntro(270, 'PERMISSIONS', '权限控制', '管理任务文件、网络与 AI 审批策略。');
  body += card(270, 248, 1140, 496, `${text(294, 280, '本机权限控制', 15, 700)}${text(294, 306, '默认只读写授权项目；联网或访问项目外文件时按策略审批。', 12, 400, C.secondary)}${button(1228, 270, '恢复推荐设置', false, 130)}${line(294, 336, 1386, 336)}${text(294, 374, '任务运行方式', 11, 600, C.secondary)}${pill(500, 358, '本机进程隔离', C.greenSoft, C.green, 102)}${text(294, 422, '本地文件访问', 11, 600, C.secondary)}${rect(500, 400, 360, 38, C.panel, 8, C.line)}${text(516, 424, '仅授权项目（推荐）⌄', 11, 500)}${text(294, 478, '互联网访问', 11, 600, C.secondary)}${pill(500, 462, '默认关闭', '#EEEFF2', C.secondary, 72)}${text(590, 478, '确需联网时由审批模型决定批准、拒绝或转交给你', 11, 400, C.secondary)}${text(294, 534, '联网模式', 11, 600, C.secondary)}${rect(500, 512, 360, 38, C.panel, 8, C.line)}${text(516, 536, '默认关闭 ⌄', 11, 500)}${text(294, 590, 'AI 自动审批', 11, 600, C.secondary)}${pill(500, 574, '开启', C.blueSoft, C.blue, 54)}${text(572, 590, '同时适用于联网和项目外文件临时访问', 11, 400, C.secondary)}${text(294, 646, '技术信息', 11, 650, C.secondary)}${text(294, 678, '▸ capability、backend 与安全策略详情', 11, 500)}${text(294, 714, '▸ 高级运行信息 · 当前本机任务租约', 11, 500)}`);
  body += card(270, 768, 1140, 74, `${circle(294, 805, 6, C.green)}${text(314, 809, '当前没有运行中的本机任务租约', 12, 500, C.secondary)}${button(1294, 789, '刷新', false, 84)}`);
  write('19-connector-sandbox.svg', connectorShell('权限控制', body));
}

// 20 系统上下文
{
  let body = rect(244, 102, 1196, 798, '#FAFAFB') + rect(244, 102, 330, 798, '#F8F8FA') + line(574, 102, 574, 900);
  body += text(268, 142, '系统提示词管理', 20, 720) + button(268, 166, '新建提示词', true, 104) + rect(380, 166, 170, 32, C.panel, 8, C.line) + text(396, 187, '⌕ 搜索提示词', 11, 400, C.tertiary) + text(268, 244, '暂无提示词', 12, 500, C.secondary);
  body += text(598, 140, '提示词工作区', 17, 700) + pill(598, 158, 'AI 生成 / 优化 / 评估', C.purpleSoft, C.purple, 146) + button(1284, 140, '保存', true, 84);
  body += text(598, 214, '名称', 11, 600, C.secondary) + rect(598, 228, 360, 38, C.panel, 8, C.line) + text(614, 252, '例如：编程助手', 11, 400, C.tertiary);
  body += text(982, 214, 'AI 场景 / 风格 / 语言', 11, 600, C.secondary) + pill(982, 228, '专业、简洁', '#EEEFF2', C.secondary, 88) + pill(1076, 228, '中文', '#EEEFF2', C.secondary, 54) + pill(1136, 228, '结构化输出', '#EEEFF2', C.secondary, 88);
  body += text(598, 304, 'AI 约束（每行一条）', 11, 600, C.secondary) + rect(598, 318, 360, 90, C.panel, 8, C.line) + text(614, 344, '例如：先给结论', 11, 400, C.tertiary);
  body += text(982, 304, 'AI 禁止项（每行一条）', 11, 600, C.secondary) + rect(982, 318, 360, 90, C.panel, 8, C.line) + text(998, 344, '例如：不要编造来源', 11, 400, C.tertiary);
  body += text(598, 448, '优化目标', 11, 600, C.secondary) + rect(598, 462, 744, 38, C.panel, 8, C.line) + text(614, 486, '提升约束完整性与可执行性', 11, 500);
  body += text(598, 540, '提示词内容', 11, 600, C.secondary) + text(1324, 540, '0 字符', 10, 400, C.tertiary, 'end') + rect(598, 554, 744, 208, C.panel, 8, C.line) + text(614, 582, '在这里编写或让 AI 生成系统提示词内容…', 11, 400, C.tertiary);
  body += button(598, 784, 'AI 生成', false, 92) + button(698, 784, 'AI 优化', false, 92) + button(798, 784, 'AI 评估', false, 92);
  write('20-system-context.svg', resourceShell('系统上下文', '', body));
}

// 21 用户偏好
{
  let body = rect(250, 124, 940, 652, C.window, 16, C.line) + rect(250, 124, 940, 52, '#F7F7F8', 16) + rect(250, 160, 940, 16, '#F7F7F8') + line(250, 176, 1190, 176);
  body += circle(274, 150, 6, '#FF5F57') + circle(294, 150, 6, '#FEBB2E') + circle(314, 150, 6, '#28C840') + text(720, 156, 'ChatOS 设置', 13, 680, C.text, 'middle');
  body += rect(250, 176, 226, 600, '#F2F2F4') + line(476, 176, 476, 776);
  body += text(274, 212, '设置', 18, 720) + rect(270, 234, 186, 36, '#DDEBFF', 8) + icon(280, 240, 'Aa', C.blueSoft, C.blue, 24) + text(316, 258, '常规', 12, 650, C.blue);
  body += icon(280, 286, '✦', C.purpleSoft, C.purple, 24) + text(316, 304, '云端 AI', 12, 550) + line(270, 334, 456, 334);
  body += text(280, 366, '账号', 10, 700, C.tertiary) + circle(292, 398, 16, '#E0E3E9') + text(292, 403, 'L', 10, 700, C.secondary, 'middle') + text(318, 394, '设计账号', 12, 650) + text(318, 412, '当前登录账号', 10, 450, C.secondary);
  body += text(518, 224, '常规', 24, 720) + text(518, 252, '界面与客户端内部上下文语言。', 12, 450, C.secondary);
  body += text(518, 312, '语言', 11, 700, C.tertiary) + line(518, 326, 1146, 326);
  body += text(518, 366, '界面语言', 13, 550) + rect(900, 346, 246, 36, C.panel, 8, C.line) + text(916, 369, '中文', 12, 500) + text(1128, 369, '⌄', 11, 600, C.secondary, 'middle');
  body += text(518, 402, '影响菜单、按钮、弹窗和状态提示，不改写用户内容。', 11, 450, C.secondary) + line(518, 426, 1146, 426);
  body += text(518, 466, '内部上下文语言', 13, 550) + rect(900, 446, 246, 36, C.panel, 8, C.line) + text(916, 469, '中文', 12, 500) + text(1128, 469, '⌄', 11, 600, C.secondary, 'middle');
  body += text(518, 502, '只影响客户端生成的内部上下文；工具输出和压缩记忆原文保持不变。', 11, 450, C.secondary);
  body += text(518, 570, '保存方式', 11, 700, C.tertiary) + line(518, 584, 1146, 584) + text(518, 624, '更改会自动保存到当前账号。', 13, 550) + pill(1038, 608, '已保存', C.greenSoft, C.green, 78);
  write('21-user-preferences.svg', body);
}

// 22 本地终端资源
{
  let body = rect(244, 102, 1196, 798, '#FCFCFD');

  // Open terminal tabs and contextual actions.
  body += rect(244, 102, 1196, 46, '#F4F4F6') + line(244, 148, 1440, 148);
  body += rect(258, 110, 194, 32, C.panel, 8, C.line) + circle(274, 126, 4, C.green) + text(288, 130, 'test_project', 12, 650) + text(430, 130, '×', 14, 500, C.secondary, 'middle');
  body += text(470, 132, '+', 18, 500, C.secondary, 'middle');
  body += button(1260, 109, '⌕  搜索', false, 72) + text(1364, 131, '•••', 15, 650, C.secondary, 'middle');

  // Terminal header uses the selected session's real cwd and connection state.
  body += rect(244, 148, 1196, 50, '#FAFAFB') + line(244, 198, 1440, 198);
  body += text(270, 179, '▱', 13, 600, C.secondary) + text(294, 179, '~/Projects/test_project', 12, 550, C.text, 'start', mono);
  body += circle(1260, 173, 4, C.green) + text(1272, 178, '已连接', 11, 550, C.secondary) + pill(1340, 161, 'zsh', '#EEEFF2', C.secondary, 50);

  // Light terminal canvas. ANSI colors remain semantic but the surface follows the app theme.
  body += rect(244, 198, 1196, 672, C.panel) + button(774, 214, '↑ 加载更早输出', false, 138);
  body += text(274, 274, 'Last login: today at 10:31 on ttys001', 12, 450, C.tertiary, 'start', mono);
  const terminalLines = [
    [310, '~/Projects/test_project', C.blue, ' % ', C.green, 'pwd', C.text],
    [346, '', C.blue, '', C.green, '~/Projects/test_project', C.text],
    [398, '~/Projects/test_project', C.blue, ' % ', C.green, 'git status --short', C.text],
    [434, '', C.blue, '', C.green, ' M frontend/src/App.tsx', C.orange],
    [470, '', C.blue, '', C.green, ' M docs/terminal-design.md', C.orange],
    [522, '~/Projects/test_project', C.blue, ' % ', C.green, 'npm test -- --run', C.text],
    [558, '', C.blue, '', C.green, '✓ 42 tests passed in 3.8s', C.green],
    [610, '~/Projects/test_project', C.blue, ' % ', C.green, '', C.text],
  ];
  terminalLines.forEach(([y, prompt, promptColor, separator, separatorColor, value, valueColor]) => {
    body += text(274, y, prompt, 12, 550, promptColor, 'start', mono);
    const separatorX = prompt ? 452 : 274;
    body += text(separatorX, y, separator, 12, 650, separatorColor, 'start', mono);
    body += text(separatorX + (separator ? 26 : 0), y, value, 12, 450, valueColor, 'start', mono);
  });
  body += rect(478, 596, 2, 18, C.text);

  // A quiet status bar replaces the oversized session-status card.
  body += rect(244, 870, 1196, 30, '#F4F4F6') + line(244, 870, 1440, 870);
  body += circle(264, 885, 4, C.green) + text(276, 889, '已连接', 10, 550, C.secondary) + text(338, 889, 'Local Connector', 10, 500, C.secondary) + text(452, 889, 'zsh', 10, 500, C.secondary) + text(496, 889, '116 × 38', 10, 500, C.secondary) + text(1416, 889, 'UTF-8', 10, 500, C.secondary, 'end');
  write('22-local-terminal.svg', resourceShell('test_project', 'terminal:test_project', body));
}

console.log(`Generated 22 SVG mockups in ${outDir}`);
