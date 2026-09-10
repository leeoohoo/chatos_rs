const token = document.querySelector('meta[name="planning-session"]').content;
const $ = id => document.getElementById(id);
let state;
let tab = 'requirements';
let selected = null;
let creating = false;
let editing = false;
let busy = false;
let pending = null;
let baseline = '';
let hostReady = false;
const taskStatus = new Map();
const statusLoads = new Set();

const titles = {
  requirements: '需求', documents: '规划文档', workItems: '工作项', relations: '关系与范围',
  plans: '规划版本', executions: '执行交接'
};
const labels = {
  draft: '草稿', reviewing: '待审核', approved: '已批准', accepted: '已验收', archived: '已归档',
  todo: '待细化', ready: '就绪', published: '已发布', prepared: '待确认', markdown: 'Markdown', svg: 'SVG',
  queued: '排队中', running: '运行中', succeeded: '已完成', failed: '失败', blocked: '阻塞', cancelled: '已取消',
  technical: '技术方案', design: '交互设计', api: '接口设计', decision: '决策记录', notes: '项目笔记',
  batch: '任务批次', task: '任务', run: '运行'
};
const paths = {
  requirements: ['M8 5H5v15h14V5h-3', 'M9 3h6v4H9z', 'M8 12h8M8 16h5'],
  documents: ['M6 3h8l4 4v14H6z', 'M14 3v5h4', 'M9 12h6M9 16h6'],
  workItems: ['M4 6l2 2 3-4M12 6h8M4 13l2 2 3-4M12 13h8M4 20l2 2 3-4M12 20h8'],
  relations: ['M5 5h5v5H5zM14 14h5v5h-5z', 'M10 7.5h4a3 3 0 0 1 3 3V14M7.5 10v4a3 3 0 0 0 3 3H14'],
  plans: ['M4 4h6v6H4zM14 14h6v6h-6z', 'M10 7h7v7M7 10v7h7'],
  executions: ['M5 12h12', 'M13 8l4 4-4 4', 'M5 5h14v14H5z'],
  folder: ['M3 6h6l2 2h10v11H3z'],
  refresh: ['M20 10a8 8 0 1 0-2 8', 'M20 4v6h-6'],
  plus: ['M12 5v14M5 12h14'],
  search: ['M21 21l-5-5', 'M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0'],
  disk: ['M4 4h14l2 2v14H4zM8 4v6h8V4M8 20v-7h8v7'],
  chevron: ['M9 6l6 6-6 6']
};

function node(tag, text, className) {
  const element = document.createElement(tag);
  if (text !== undefined) element.textContent = text;
  if (className) element.className = className;
  return element;
}

function icon(name) {
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  for (const [key, value] of Object.entries({
    viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-linecap': 'round',
    'stroke-linejoin': 'round', class: 'icon', 'aria-hidden': 'true'
  })) svg.setAttribute(key, value);
  for (const d of paths[name] ?? paths.documents) {
    const segment = document.createElementNS(svg.namespaceURI, 'path');
    segment.setAttribute('d', d);
    svg.append(segment);
  }
  return svg;
}

function button(text, action, primary = false) {
  const element = node('button', text, primary ? 'primary' : '');
  element.type = 'button';
  element.onclick = action;
  return element;
}

function badge(value) { return node('span', labels[value] ?? value, `badge ${value}`); }

function ask(title, body, accept = '确认') {
  const dialog = $('confirm-dialog');
  $('confirm-title').textContent = title;
  $('confirm-body').textContent = body;
  $('confirm-accept').textContent = accept;
  dialog.returnValue = '';
  return new Promise(resolve => {
    dialog.addEventListener('close', () => resolve(dialog.returnValue === 'confirm'), { once: true });
    dialog.showModal();
  });
}

function fingerprint(root = $('editor')) {
  return JSON.stringify([...root.querySelectorAll('input,textarea,select')].map(element => [
    element.name, element.type === 'checkbox' ? element.checked : element.value
  ]));
}
function isDirty() { return fingerprint() !== baseline; }
function remember() { baseline = fingerprint(); }
function updateStatus() {
  $('save-status').textContent = busy ? '正在保存…' : pending ? '保存待确认' : !state ? '尚未加载' : isDirty() ? '有未保存的更改' : '已保存到本机';
}

async function mayLeave() {
  if (busy) return false;
  if (pending) { message('上一次保存结果尚未确认，请先重试。'); return false; }
  return !isDirty() || await ask('放弃未保存的更改？', '当前编辑尚未保存，离开后会丢失。', '放弃更改');
}

async function api(path, value) {
  let response;
  try {
    response = await fetch(path, {
      method: value ? 'POST' : 'GET',
      signal: AbortSignal.timeout(15_000),
      headers: { 'x-planning-session': token, ...(value ? { 'Content-Type': 'application/json' } : {}) },
      ...(value ? { body: JSON.stringify(value) } : {})
    });
  } catch { throw new Error('暂时无法连接本地插件'); }
  const result = await response.json();
  if (!response.ok) {
    const error = new Error(errorText(result.error));
    error.status = response.status;
    throw error;
  }
  return result;
}

function errorText(reason) {
  const messages = {
    'Planning data changed; reload before editing': '资料已被其他窗口或工具更新',
    'Acceptance criteria are required': '请先填写验收标准',
    'A published planning document is required': '请先关联并发布一份规划文档',
    'A published planning document is required before readiness': '请先为所属需求关联并发布规划文档',
    'Dependency or requirement hierarchy contains a cycle': '父子关系或依赖关系形成循环，请调整关联',
    'Dependency target not found or archived': '前置资料不存在或已归档',
    'Dependency source not found or archived': '当前资料不存在或已归档',
    'Plan work items and their prerequisites must be ready': '选中工作项及其全部前置工作项必须就绪',
    'Plan requirements and their prerequisites must be approved': '相关需求及其前置需求必须已批准',
    'Plan is missing a published planning document': '规划范围中有需求缺少已发布文档',
    'Archive child requirements and work items first': '请先归档子需求和所属工作项',
    'Requirement is still a prerequisite': '其他需求仍依赖此需求',
    'Work item is still a prerequisite': '其他工作项仍依赖此工作项',
    'Reopen the requirement as draft before revising approved content': '请先将需求退回草稿，再修改已批准内容',
    'Reopen approved linked requirements before revising their document': '请先将关联的已批准需求退回草稿，再修订文档',
    'Only an approved frozen plan can prepare execution': '只有已批准的冻结规划才能准备执行',
    'Execution references require an approved intent': '请先确认执行意图，再关联 Task Runner',
    'Unknown or invalid planning fields': '填写内容不符合要求，请检查必填项和关联范围',
    'Invalid application session': '页面会话已失效，请重新打开插件',
    'Local planning data could not be read or saved': '本地资料读取或保存失败',
    'Document revision does not contain any changes': '文档没有任何可保存的更改'
  };
  return messages[reason] ?? reason ?? '操作未完成，请核对资料后重试';
}

function hostRequest(method, payload) {
  if (!window.chatosHost) throw new Error('请从 ChatOS 客户端的“应用”中打开此插件');
  return window.chatosHost.request(method, payload);
}

function message(text = '') {
  $('message').replaceChildren();
  $('message').hidden = !text;
  if (!text) return;
  $('message').append(node('span', text));
  $('message').append(pending ? button('重试保存', () => change()) : button('关闭提示', () => message()));
}

function availability() {
  document.querySelectorAll('button,input,textarea,select').forEach(element => {
    if (element.closest('dialog')) return;
    const locked = busy || (pending && !element.closest('#message'));
    if (locked && !element.hasAttribute('data-runtime-disabled')) {
      element.dataset.runtimeDisabled = String(element.disabled);
      element.disabled = true;
    } else if (!locked && element.hasAttribute('data-runtime-disabled')) {
      element.disabled = element.dataset.runtimeDisabled === 'true';
      delete element.dataset.runtimeDisabled;
    }
  });
  $('create').disabled = busy || !!pending || !state;
  $('search').disabled = busy || !!pending || !state;
  $('editor').setAttribute('aria-busy', String(busy));
}

function relationItems() {
  return [
    ...state.requirements.map(item => ({ ...item, relationKind: 'requirement', sourceId: item.id, id: `requirement:${item.id}` })),
    ...state.workItems.map(item => ({ ...item, relationKind: 'work_item', sourceId: item.id, id: `work_item:${item.id}` }))
  ];
}

function sourceItems() {
  if (!state) return [];
  if (tab === 'relations') return relationItems();
  if (tab === 'executions') return state.executionIntents;
  return state[tab];
}

async function load() {
  state = await api('/api/state');
  $('project-name').textContent = state.context.projectName;
  $('project-name').title = state.context.projectName;
  $('revision').textContent = `资料版本 ${state.revision}`;
  const items = sourceItems();
  if (!creating && !items.some(item => item.id === selected)) selected = items[0]?.id ?? null;
  render();
}

async function change(fields) {
  if (busy) return;
  const command = pending ?? { ...fields, expectedRevision: state.revision, requestId: crypto.randomUUID() };
  busy = true;
  pending = command;
  message(); availability(); updateStatus();
  let committed = false;
  try {
    const result = await api('/api/changes', command);
    committed = true;
    pending = null;
    selected = result.result.id;
    creating = false;
    editing = false;
    await load();
  } catch (error) {
    if (error.status) pending = null;
    if (committed) {
      $('editor').replaceChildren();
      empty($('editor'), '保存成功，列表刷新失败', '更改已经安全保存，请刷新资料。');
      remember();
      message('资料已保存，但列表刷新失败。');
    } else {
      const suffix = error.status === 409 ? '。当前编辑已保留，请刷新后核对。' : !error.status ? '。保存结果尚未确认，重试不会重复创建。' : '';
      message(error.message + suffix);
    }
  } finally {
    busy = false;
    availability();
    updateStatus();
  }
}

function empty(editor, title, detail, action) {
  const section = node('div', undefined, 'empty-state');
  section.append(icon(tab), node('h2', title), node('p', detail));
  if (action) section.append(action);
  editor.append(section);
}

function meta(editor, title, status) {
  const row = node('div', undefined, 'detail-meta');
  row.append(icon(tab), node('span', title));
  if (status) row.append(badge(status));
  editor.append(row);
}

function field(form, name, title, value = '', options) {
  const label = node('label', undefined, `field field-${name}`);
  label.append(node('span', title));
  let input;
  if (options) {
    input = node('select');
    for (const [optionValue, optionTitle] of options) {
      const option = node('option', optionTitle);
      option.value = optionValue;
      input.append(option);
    }
  } else {
    input = node(['detail', 'acceptanceCriteria', 'content'].includes(name) ? 'textarea' : 'input');
  }
  input.name = name;
  input.value = value ?? '';
  if (name === 'title') { input.required = true; input.maxLength = 240; input.placeholder = `输入${title}`; }
  if (input.tagName === 'TEXTAREA') input.maxLength = 200_000;
  if (name === 'detail') input.placeholder = '描述目标、背景、边界与不包含内容…';
  if (name === 'acceptanceCriteria') input.placeholder = '写下可以验证的完成条件…';
  label.append(input);
  form.append(label);
  return input;
}

function formSave(form, extract, caption = '保存更改') {
  const actions = node('div', undefined, 'actions');
  const submit = node('button', caption, 'primary');
  submit.type = 'submit';
  actions.append(button('取消', async () => {
    if (!await mayLeave()) return;
    editing = false; creating = false;
    selected ??= sourceItems()[0]?.id ?? null;
    message(); render();
  }), submit);
  form.append(actions);
  form.onsubmit = async event => {
    event.preventDefault();
    const values = extract(new FormData(form));
    if (values.status === 'archived' && !await ask('归档这项资料？', '归档后内容将只读，仍被有效资料依赖时会拒绝归档。', '归档')) return;
    if (values.status === 'accepted' && !await ask('确认业务验收通过？', '业务验收不会修改 Task Runner 的运行状态。', '确认验收')) return;
    await change(values);
  };
}

function checklist(items, selectedIds = [], caption) {
  const list = node('div', undefined, 'checklist');
  for (const item of items) {
    const label = node('label', undefined, 'check-row');
    const input = node('input');
    input.type = 'checkbox'; input.name = 'selectedIds'; input.value = item.id; input.checked = selectedIds.includes(item.id);
    const copy = node('span', item.title);
    if (caption) copy.append(node('small', caption(item)));
    label.append(input, copy);
    list.append(label);
  }
  return list;
}
function selectedIds(list) { return [...list.querySelectorAll('input:checked')].map(input => input.value); }

function requirementPath(item) {
  const path = [];
  const seen = new Set();
  let current = item;
  while (current && !seen.has(current.id)) {
    path.unshift(current);
    seen.add(current.id);
    current = state.requirements.find(value => value.id === current.parentId);
  }
  return path;
}

function renderList() {
  const items = sourceItems();
  const query = $('search').value.trim().toLocaleLowerCase();
  const filtered = items.filter(item => `${item.title} ${item.detail ?? ''}`.toLocaleLowerCase().includes(query));
  $('list-count').textContent = query ? `${filtered.length} / ${items.length}` : `${items.length} 项`;
  $('items').replaceChildren();
  if (!filtered.length) $('items').append(node('p', query ? '没有找到匹配的资料' : `还没有${titles[tab]}`, 'list-empty'));

  const depth = item => {
    if (tab !== 'requirements') return 0;
    return Math.max(0, requirementPath(item).length - 1);
  };
  const sorted = tab === 'requirements'
    ? [...filtered].sort((left, right) => requirementPath(left).map(value => value.title).join('/').localeCompare(requirementPath(right).map(value => value.title).join('/')))
    : filtered;
  for (const item of sorted) {
    const row = button('', async () => {
      if ((selected === item.id && !creating) || !await mayLeave()) return;
      selected = item.id; creating = false; editing = false; message(); render();
    });
    row.className = `item${item.id === selected && !creating ? ' selected' : ''}`;
    row.style.setProperty('--tree-depth', depth(item));
    if (item.id === selected && !creating) row.setAttribute('aria-current', 'true');
    const copy = node('span', undefined, 'item-copy');
    const subtitle = tab === 'documents'
      ? `${labels[item.kind]} · v${item.currentVersion} · ${labels[item.status]}`
      : tab === 'relations'
        ? item.relationKind === 'requirement' ? '需求关系' : '工作项关系'
        : tab === 'executions'
          ? `${labels[item.status]} · 规划 v${state.plans.find(value => value.id === item.planId)?.version ?? '—'}`
          : `${item.version ? `版本 ${item.version} · ` : ''}${labels[item.status]}`;
    copy.append(node('strong', item.title), node('small', subtitle));
    if (item.detail) copy.append(node('span', item.detail, 'excerpt'));
    row.append(icon(tab), copy);
    if (tab === 'requirements' && state.requirements.some(value => value.parentId === item.id)) row.append(icon('chevron'));
    $('items').append(row);
  }
}

function render() {
  document.querySelectorAll('[data-tab]').forEach(element => {
    element.removeAttribute('aria-current');
    if (element.dataset.tab === tab) element.setAttribute('aria-current', 'page');
  });
  document.querySelectorAll('[data-count]').forEach(element => {
    element.textContent = state[element.dataset.count]?.length ?? 0;
  });
  $('list-title').textContent = titles[tab];
  $('toolbar-section').textContent = titles[tab];
  $('search').placeholder = `搜索${titles[tab]}`;
  $('create').hidden = tab === 'relations';
  $('create-label').textContent = tab === 'plans' ? '新建规划' : tab === 'executions' ? '准备执行' : `新建${titles[tab]}`;
  renderList();

  const item = creating ? null : sourceItems().find(value => value.id === selected);
  const editor = $('editor');
  editor.replaceChildren();
  if (!item && !creating) {
    const action = tab === 'relations' ? undefined : button(tab === 'executions' ? '准备执行' : tab === 'plans' ? '新建规划' : `新建${titles[tab]}`, startCreate, true);
    empty(editor, `还没有${titles[tab]}`, emptyDetail(), action);
  } else if (tab === 'requirements' || tab === 'workItems') renderEntity(editor, item);
  else if (tab === 'documents') renderDocument(editor, item);
  else if (tab === 'relations') renderRelations(editor, item);
  else if (tab === 'plans') renderPlan(editor, item);
  else renderExecution(editor, item);
  remember(); availability(); updateStatus();
}

function emptyDetail() {
  if (tab === 'documents') return '文档拥有独立身份、类型和不可变版本，可关联一个或多个需求。';
  if (tab === 'workItems') return '工作项承载可执行范围和验收标准，运行状态由 Task Runner 管理。';
  if (tab === 'plans') return '从已批准需求和就绪工作项创建冻结规划版本。';
  if (tab === 'executions') return '批准冻结规划后，可准备并确认一次执行交接。';
  return '从一项清晰的目标开始，逐步建立项目范围。';
}

function renderDependencies(form, item, isRequirement, locked) {
  const kind = isRequirement ? 'requirement' : 'work_item';
  const edges = state.dependencies.filter(value => value.kind === kind && value.id === item.id);
  const disclosure = node('details', undefined, 'dependencies');
  const body = node('div');
  disclosure.append(node('summary', `前置依赖 · ${edges.length} 项`), body);
  const candidates = (isRequirement ? state.requirements : state.workItems).filter(value => value.id !== item.id && value.status !== 'archived');
  let list = null;
  if (!candidates.length) body.append(node('p', '暂无可关联资料。', 'hint'));
  else {
    list = checklist(candidates, edges.map(value => value.prerequisiteId));
    body.append(list, node('p', locked ? '将需求退回草稿后才能修改依赖。' : '勾选必须先完成的资料，系统会阻止依赖环。', 'hint'));
    if (locked) list.querySelectorAll('input').forEach(input => { input.disabled = true; });
  }
  form.append(disclosure);
  return list;
}

function renderEntity(editor, item) {
  const isRequirement = tab === 'requirements';
  meta(editor, `${item ? '' : '新建'}${titles[tab]}`, item?.status ?? (isRequirement ? 'draft' : 'todo'));
  if (!isRequirement && !item && !state.requirements.some(value => value.status !== 'archived')) {
    empty(editor, '先创建需求', '每个工作项必须归属于一个需求。', button('前往需求', () => switchTab('requirements'), true));
    return;
  }
  if (item && (!editing || item.status === 'archived')) {
    if (isRequirement) {
      const breadcrumb = node('div', undefined, 'content-breadcrumb');
      requirementPath(item).forEach((value, index, values) => {
        const link = button(value.title, () => { selected = value.id; render(); });
        link.className = 'link-button'; breadcrumb.append(link);
        if (index < values.length - 1) breadcrumb.append(node('span', '›'));
      });
      editor.append(breadcrumb);
    }
    editor.append(node('h2', item.title));
    const actions = node('div', undefined, 'actions reading-actions');
    if (item.status !== 'archived') actions.append(button(`编辑${titles[tab]}`, () => { editing = true; render(); }, true));
    else actions.append(node('p', '已归档，仅供查阅。', 'hint'));
    editor.append(actions);
    if (!isRequirement) {
      const parent = state.requirements.find(value => value.id === item.requirementId);
      if (parent) editor.append(linkCard('所属需求', parent.title, () => { tab = 'requirements'; selected = parent.id; render(); }));
    }
    editor.append(section('说明与范围', item.detail || '未填写'), section('验收标准', item.acceptanceCriteria || '未填写'));
    const kind = isRequirement ? 'requirement' : 'work_item';
    const source = isRequirement ? state.requirements : state.workItems;
    const prerequisites = state.dependencies.filter(value => value.kind === kind && value.id === item.id).map(value => source.find(target => target.id === value.prerequisiteId)).filter(Boolean);
    const dependants = state.dependencies.filter(value => value.kind === kind && value.prerequisiteId === item.id).map(value => source.find(target => target.id === value.id)).filter(Boolean);
    editor.append(relationshipStrip(prerequisites, dependants));
    if (isRequirement) renderRequirementResources(editor, item);
    const relationId = `${kind}:${item.id}`;
    editor.append(button('查看完整关系与范围', () => { tab = 'relations'; selected = relationId; editing = false; render(); }));
    return;
  }

  const form = node('form');
  const locked = isRequirement && ['approved', 'accepted'].includes(item?.status);
  field(form, 'title', '标题', item?.title);
  if (isRequirement) {
    field(form, 'parentId', '父需求', item?.parentId, [['', '无父需求'], ...state.requirements.filter(value => value.id !== item?.id && value.status !== 'archived').map(value => [value.id, value.title])]);
  } else {
    const options = state.requirements.filter(value => value.status !== 'archived' || value.id === item?.requirementId).map(value => [value.id, value.title]);
    const select = field(form, 'requirementId', '所属需求', item?.requirementId ?? options[0]?.[0], options);
    if (item) select.disabled = true;
  }
  const statuses = item
    ? isRequirement ? ['draft', 'reviewing', 'approved', 'accepted', 'archived'] : ['todo', 'ready', 'accepted', 'archived']
    : [isRequirement ? 'draft' : 'todo'];
  field(form, 'status', '业务状态', item?.status ?? statuses[0], statuses.map(value => [value, labels[value]]));
  field(form, 'detail', '说明与范围', item?.detail);
  field(form, 'acceptanceCriteria', '验收标准', item?.acceptanceCriteria);
  if (locked) {
    form.querySelectorAll('input,textarea,select:not([name=status])').forEach(input => { input.disabled = true; });
    form.append(node('p', '已批准内容不可直接修改，请先将状态退回草稿并保存。', 'hint detail-note'));
  }
  const dependencyList = item ? renderDependencies(form, item, isRequirement, locked) : null;
  formSave(form, data => ({
    operation: `${isRequirement ? 'requirement' : 'work_item'}.${item ? 'update' : 'create'}`,
    ...(item ? { id: item.id } : {}),
    title: locked ? item.title : data.get('title'),
    detail: locked ? item.detail : data.get('detail'),
    acceptanceCriteria: locked ? item.acceptanceCriteria : data.get('acceptanceCriteria'),
    status: data.get('status'),
    ...(isRequirement
      ? { parentId: locked ? item.parentId : data.get('parentId') || null }
      : { requirementId: item?.requirementId ?? data.get('requirementId') }),
    ...(dependencyList && !locked ? { prerequisiteIds: selectedIds(dependencyList) } : {})
  }), item ? '保存更改' : `创建${titles[tab]}`);
  editor.append(form);
}

function section(title, value) {
  const container = node('section', undefined, 'reading-section');
  container.append(node('h3', title), node('p', value, 'prose'));
  return container;
}

function linkCard(eyebrow, title, action) {
  const card = button('', action);
  card.className = 'link-card';
  const copy = node('span'); copy.append(node('small', eyebrow), node('strong', title));
  card.append(copy, icon('chevron'));
  return card;
}

function relationshipStrip(prerequisites, dependants) {
  const strip = node('div', undefined, 'relation-summary');
  const make = (title, items) => {
    const group = node('div');
    group.append(node('small', title), node('strong', String(items.length)));
    if (items.length) group.append(node('span', items.slice(0, 3).map(value => value.title).join('、')));
    else group.append(node('span', '无'));
    return group;
  };
  strip.append(make('前置', prerequisites), make('后续', dependants));
  return strip;
}

function renderRequirementResources(editor, requirement) {
  const children = state.requirements.filter(value => value.parentId === requirement.id);
  const documents = state.documentLinks.filter(value => value.requirementId === requirement.id).map(value => state.documents.find(document => document.id === value.documentId)).filter(Boolean);
  const workItems = state.workItems.filter(value => value.requirementId === requirement.id);
  const grid = node('div', undefined, 'resource-grid');
  for (const child of children) grid.append(linkCard('子需求', child.title, () => { selected = child.id; render(); }));
  for (const document of documents) grid.append(linkCard(`${labels[document.kind]} · v${document.currentVersion}`, document.title, () => { tab = 'documents'; selected = document.id; render(); }));
  for (const workItem of workItems) grid.append(linkCard(`工作项 · ${labels[workItem.status]}`, workItem.title, () => { tab = 'workItems'; selected = workItem.id; render(); }));
  editor.append(node('h3', `相关资料 · ${grid.children.length}`));
  if (grid.children.length) editor.append(grid); else editor.append(node('p', '尚未关联子需求、文档或工作项。', 'hint'));
}

function renderDocument(editor, item) {
  meta(editor, item ? `${labels[item.kind]} · v${item.currentVersion}` : '新建规划文档', item?.status ?? 'draft');
  if (item && !editing) {
    editor.append(node('h2', item.title));
    const links = state.documentLinks.filter(value => value.documentId === item.id).map(value => state.requirements.find(requirement => requirement.id === value.requirementId)).filter(Boolean);
    const actions = node('div', undefined, 'actions reading-actions');
    if (item.status !== 'archived') actions.append(button('修订文档', () => { editing = true; render(); }, true));
    editor.append(actions);
    const detail = getDocumentDetail(item.id);
    if (!detail) {
      editor.append(node('p', '正在读取文档版本…', 'hint'));
      return;
    }
    const versionSelect = node('select');
    versionSelect.className = 'version-picker';
    for (const version of detail.versions) {
      const option = node('option', `版本 ${version.version} · ${new Date(version.createdAt).toLocaleString()}`);
      option.value = version.id; versionSelect.append(option);
    }
    const viewer = node('article', undefined, 'document-viewer');
    const show = () => {
      const version = detail.versions.find(value => value.id === versionSelect.value) ?? detail.versions[0];
      viewer.replaceChildren();
      viewer.append(item.format === 'svg' ? renderSafeSvg(version.content) : renderMarkdown(version.content));
    };
    versionSelect.onchange = show; show();
    const linked = node('div', undefined, 'chip-row');
    links.forEach(value => linked.append(badgeWithText(value.title)));
    editor.append(node('h3', '关联需求'), linked, node('h3', '版本历史'), versionSelect, viewer);
    return;
  }

  if (!item && !state.requirements.some(value => value.status !== 'archived')) {
    empty(editor, '先创建需求', '规划文档至少要关联一个需求。', button('前往需求', () => switchTab('requirements'), true));
    return;
  }
  const detail = item ? getDocumentDetail(item.id) : null;
  const form = node('form');
  field(form, 'title', '文档标题', item?.title);
  field(form, 'kind', '文档类型', item?.kind ?? 'technical', ['technical', 'design', 'api', 'decision', 'notes'].map(value => [value, labels[value]]));
  field(form, 'format', '内容格式', item?.format ?? 'markdown', [['markdown', 'Markdown'], ['svg', 'SVG 图形']]);
  field(form, 'status', '文档状态', item?.status ?? 'draft', ['draft', 'published', ...(item ? ['archived'] : [])].map(value => [value, labels[value]]));
  const currentLinks = item ? state.documentLinks.filter(value => value.documentId === item.id).map(value => value.requirementId) : [];
  const requirements = checklist(state.requirements.filter(value => value.status !== 'archived'), currentLinks, value => labels[value.status]);
  const fieldset = node('fieldset'); fieldset.append(node('legend', '关联需求'), requirements); form.append(fieldset);
  const content = field(form, 'content', '文档内容', detail?.versions[0]?.content ?? '');
  content.rows = 20; content.required = true;
  content.placeholder = '使用 Markdown 编写说明，或选择 SVG 后粘贴不含脚本和外部资源的图形…';
  formSave(form, data => ({
    operation: item ? 'document.revise' : 'document.create',
    ...(item ? { id: item.id } : {}),
    title: data.get('title'), kind: data.get('kind'), format: data.get('format'), status: data.get('status'),
    requirementIds: selectedIds(requirements), content: data.get('content')
  }), item ? '保存新版本' : '创建文档');
  editor.append(form);
}

const documentCache = new Map();
function getDocumentDetail(id) {
  if (documentCache.has(id)) return documentCache.get(id);
  api(`/api/documents/${encodeURIComponent(id)}`).then(value => {
    documentCache.set(id, value);
    if (tab === 'documents' && selected === id && !editing) render();
  }).catch(error => message(`读取文档失败：${error.message}`));
  return null;
}

function badgeWithText(text) { return node('span', text, 'badge'); }

function renderMarkdown(content) {
  const root = node('div', undefined, 'markdown-body');
  let code = null;
  for (const line of content.replaceAll('\r\n', '\n').split('\n')) {
    if (line.startsWith('```')) {
      if (code) { root.append(code); code = null; } else code = node('pre');
      continue;
    }
    if (code) { code.textContent += `${code.textContent ? '\n' : ''}${line}`; continue; }
    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    if (heading) { root.append(node(`h${heading[1].length + 1}`, heading[2])); continue; }
    const listItem = line.match(/^\s*[-*]\s+(.+)$/);
    if (listItem) {
      let list = root.lastElementChild;
      if (!list || list.tagName !== 'UL') { list = node('ul'); root.append(list); }
      list.append(node('li', listItem[1])); continue;
    }
    if (!line.trim()) { root.append(node('div', undefined, 'markdown-gap')); continue; }
    root.append(node('p', line));
  }
  if (code) root.append(code);
  return root;
}

function renderSafeSvg(content) {
  const frame = node('div', undefined, 'svg-preview');
  const parsed = new DOMParser().parseFromString(content, 'image/svg+xml');
  if (parsed.querySelector('parsererror') || parsed.documentElement.localName !== 'svg') {
    frame.append(node('p', 'SVG 格式无效，无法预览。', 'hint')); return frame;
  }
  const allowedElements = new Set(['svg', 'g', 'path', 'rect', 'circle', 'ellipse', 'line', 'polyline', 'polygon', 'text', 'tspan', 'defs', 'linearGradient', 'radialGradient', 'stop']);
  const allowedAttributes = new Set(['viewBox', 'width', 'height', 'x', 'y', 'x1', 'x2', 'y1', 'y2', 'cx', 'cy', 'r', 'rx', 'ry', 'd', 'points', 'fill', 'stroke', 'stroke-width', 'opacity', 'transform', 'font-size', 'text-anchor', 'offset', 'stop-color', 'stop-opacity', 'id']);
  const copy = source => {
    if (!allowedElements.has(source.localName)) return null;
    const target = document.createElementNS('http://www.w3.org/2000/svg', source.localName);
    for (const attribute of source.attributes) {
      if (!allowedAttributes.has(attribute.name)) continue;
      const value = attribute.value.trim();
      if (/url\s*\(|javascript:|data:|https?:/iu.test(value)) continue;
      target.setAttribute(attribute.name, value.slice(0, 20_000));
    }
    for (const child of source.childNodes) {
      if (child.nodeType === Node.TEXT_NODE && ['text', 'tspan'].includes(source.localName)) target.append(document.createTextNode(child.textContent ?? ''));
      else if (child.nodeType === Node.ELEMENT_NODE) { const safe = copy(child); if (safe) target.append(safe); }
    }
    return target;
  };
  const safe = copy(parsed.documentElement);
  if (!safe) { frame.append(node('p', 'SVG 不包含可展示内容。', 'hint')); return frame; }
  safe.setAttribute('role', 'img'); safe.setAttribute('aria-label', '文档 SVG 预览'); safe.removeAttribute('width'); safe.removeAttribute('height');
  frame.append(safe); return frame;
}

async function renderRelations(editor, item) {
  if (!item) return;
  meta(editor, item.relationKind === 'requirement' ? '需求范围' : '工作项范围');
  editor.append(node('h2', item.title), node('p', '范围由层级、前置依赖和所属关系实时计算，不代表执行授权。', 'hint detail-note'));
  const loading = node('p', '正在计算完整范围…', 'hint'); editor.append(loading);
  try {
    const graph = await api(`/api/scope?kind=${item.relationKind}&id=${encodeURIComponent(item.sourceId)}`);
    if (tab !== 'relations' || selected !== item.id) return;
    loading.remove();
    const metrics = node('div', undefined, 'metric-grid');
    for (const [label, value] of [['祖先路径', graph.ancestors.length], ['子需求', graph.descendants.length], ['全部前置', graph.prerequisites.length], ['全部后续', graph.dependants.length], ['范围工作项', graph.workItemIds.length], ['关联文档', graph.documentIds.length]]) {
      const card = node('div'); card.append(node('strong', String(value)), node('span', label)); metrics.append(card);
    }
    editor.append(metrics, node('h3', '范围关系图'), graphView(graph));
    const details = node('div', undefined, 'scope-columns');
    details.append(scopeList('祖先路径', graph.ancestors, item.relationKind), scopeList('子需求', graph.descendants, 'requirement'), scopeList('必要前置', graph.prerequisites, item.relationKind), scopeList('后续影响', graph.dependants, item.relationKind));
    editor.append(node('h3', '关系明细'), details);
  } catch (error) {
    loading.textContent = `范围计算失败：${error.message}`;
  }
}

function scopeList(title, ids, kind) {
  const section = node('section', undefined, 'scope-list'); section.append(node('h4', title));
  const source = kind === 'requirement' ? state.requirements : state.workItems;
  if (!ids.length) section.append(node('p', '无', 'hint'));
  else for (const id of ids) section.append(node('p', source.find(value => value.id === id)?.title ?? id));
  return section;
}

function graphView(graph) {
  const root = node('div', undefined, 'graph-view');
  const nodes = graph.nodes.slice(0, 80);
  const width = 760; const columnWidth = 230; const rowHeight = 68;
  const reqs = nodes.filter(value => value.kind === 'requirement');
  const works = nodes.filter(value => value.kind === 'work_item');
  const position = new Map();
  reqs.forEach((value, index) => position.set(value.id, { x: 25, y: 24 + index * rowHeight }));
  works.forEach((value, index) => position.set(value.id, { x: 25 + columnWidth * 2, y: 24 + index * rowHeight }));
  const height = Math.max(reqs.length, works.length, 2) * rowHeight + 24;
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', `0 0 ${width} ${height}`); svg.setAttribute('role', 'img'); svg.setAttribute('aria-label', '需求和工作项关系图');
  for (const edge of graph.edges) {
    const from = position.get(edge.from); const to = position.get(edge.to);
    if (!from || !to) continue;
    const line = document.createElementNS(svg.namespaceURI, 'line');
    line.setAttribute('x1', String(from.x + 190)); line.setAttribute('y1', String(from.y + 20));
    line.setAttribute('x2', String(to.x)); line.setAttribute('y2', String(to.y + 20)); line.setAttribute('class', edge.relation);
    svg.append(line);
  }
  for (const value of nodes) {
    const pos = position.get(value.id); if (!pos) continue;
    const group = document.createElementNS(svg.namespaceURI, 'g');
    const rect = document.createElementNS(svg.namespaceURI, 'rect');
    rect.setAttribute('x', String(pos.x)); rect.setAttribute('y', String(pos.y)); rect.setAttribute('width', '190'); rect.setAttribute('height', '42'); rect.setAttribute('rx', '9'); rect.setAttribute('class', value.id === graph.rootId ? 'root-node' : value.kind);
    const text = document.createElementNS(svg.namespaceURI, 'text');
    text.setAttribute('x', String(pos.x + 12)); text.setAttribute('y', String(pos.y + 25)); text.textContent = value.title.length > 22 ? `${value.title.slice(0, 21)}…` : value.title;
    group.append(rect, text); svg.append(group);
  }
  if (graph.nodes.length > nodes.length) root.append(node('p', `图中显示前 ${nodes.length} 个节点，完整范围仍用于规划校验。`, 'hint'));
  root.append(svg); return root;
}

function renderPlan(editor, plan) {
  if (plan) {
    meta(editor, `规划版本 ${plan.version}`, plan.status);
    editor.append(node('h2', plan.title), node('p', `${plan.requirements.length} 个需求 · ${plan.documents.length} 份冻结文档 · ${plan.workItems.length} 个工作项`, 'hint'));
    editor.append(node('p', '此版本已冻结具体文档版本和依赖范围，后续编辑不会改变它。', 'hint detail-note'));
    if (plan.status === 'draft') {
      const actions = node('div', undefined, 'actions');
      actions.append(node('p', '批准表示业务认可，不会启动任务。', 'hint'), button('批准规划', async () => {
        if (await ask('批准此规划版本？', '批准后可准备执行交接，但仍需单独确认执行意图。', '批准规划')) await change({ operation: 'plan.approve', id: plan.id });
      }, true)); editor.append(actions);
    } else if (!state.executionIntents.some(value => value.planId === plan.id)) {
      editor.append(button('准备执行交接', () => { tab = 'executions'; creating = true; editing = true; selected = null; render(); }, true));
    }
    editor.append(node('h3', '冻结需求'));
    for (const requirement of plan.requirements) {
      const card = node('article', undefined, 'plan-card');
      card.append(node('h4', requirement.title), node('p', requirement.detail || '未填写说明'), node('p', `验收标准：${requirement.acceptanceCriteria}`, 'hint'));
      const documents = plan.documents.filter(value => value.requirementIds.includes(requirement.id));
      for (const document of documents) {
        const details = node('details');
        const summary = node('summary', `${labels[document.kind]} · ${document.title} · v${document.pinnedVersion.version}`);
        const viewer = document.format === 'svg' ? renderSafeSvg(document.pinnedVersion.content) : renderMarkdown(document.pinnedVersion.content);
        details.append(summary, viewer); card.append(details);
      }
      editor.append(card);
    }
    editor.append(node('h3', '冻结工作项'));
    plan.workItems.forEach((workItem, index) => {
      const card = node('article', undefined, 'plan-card');
      const prerequisites = plan.dependencies.filter(value => value.kind === 'work_item' && value.id === workItem.id).map(value => plan.workItems.find(target => target.id === value.prerequisiteId)?.title).filter(Boolean);
      card.append(node('h4', `${index + 1}. ${workItem.title}`), node('p', workItem.detail), node('p', `验收标准：${workItem.acceptanceCriteria}`, 'hint'));
      if (prerequisites.length) card.append(node('p', `前置：${prerequisites.join('、')}`, 'hint'));
      editor.append(card);
    });
    return;
  }
  meta(editor, '新建规划版本', 'draft');
  const readyItems = state.workItems.filter(item => item.status === 'ready' && state.requirements.find(value => value.id === item.requirementId)?.status === 'approved');
  if (!readyItems.length) {
    empty(editor, '还没有可规划的工作项', '先发布规划文档、批准需求，再将工作项设为就绪。', button('查看工作项', () => switchTab('workItems'), true));
    return;
  }
  const form = node('form'); field(form, 'title', '规划标题');
  form.append(node('p', '选择目标工作项。插件会自动纳入全部前置工作项和前置需求，并冻结对应文档版本。', 'hint detail-note'));
  const list = checklist(readyItems, [], item => state.requirements.find(value => value.id === item.requirementId)?.title);
  const group = node('fieldset'); group.append(node('legend', '目标工作项'), list); form.append(group);
  formSave(form, data => ({ operation: 'plan.create', title: data.get('title'), workItemIds: selectedIds(list) }), '创建冻结规划');
  const submit = form.querySelector('button[type=submit]'); submit.disabled = true;
  list.addEventListener('change', () => { submit.disabled = selectedIds(list).length === 0; });
  editor.append(form);
}

function renderExecution(editor, intent) {
  if (intent) {
    const plan = state.plans.find(value => value.id === intent.planId);
    meta(editor, '执行意图', intent.status);
    editor.append(node('h2', intent.title), node('p', `来源：${plan?.title ?? '规划不可用'} · v${plan?.version ?? '—'}`, 'hint'));
    editor.append(node('p', '插件只保存交接意图和 Task Runner 不透明引用，不保存或推断运行状态。', 'hint detail-note'));
    if (intent.status === 'prepared') {
      editor.append(button('确认执行意图', async () => {
        if (await ask('确认执行意图？', '确认只允许宿主创建 Task Runner 任务；本地业务状态不会自动变成运行中。', '确认意图')) await change({ operation: 'execution.approve', id: intent.id });
      }, true));
    }
    const references = state.executionReferences.filter(value => value.intentId === intent.id);
    editor.append(node('h3', `Task Runner 引用 · ${references.length}`));
    if (!references.length) editor.append(node('p', intent.status === 'prepared' ? '确认执行意图后才能关联任务。' : '宿主尚未返回任务引用。', 'hint'));
    for (const reference of references) {
      const card = node('article', undefined, 'reference-card');
      const current = reference.referenceType === 'task' ? taskStatus.get(reference.externalId) : null;
      card.append(badge(reference.referenceType), node('strong', reference.externalId), node('span', current ? `${labels[current.status] ?? current.status} · ${current.title}` : reference.revision ? `批次修订 ${reference.revision.slice(0, 12)}` : '等待宿主状态'));
      if (reference.url) {
        const link = node('a', '在 Task Runner 中查看'); link.href = reference.url; link.rel = 'noreferrer'; card.append(link);
      }
      editor.append(card);
    }
    if (intent.status === 'approved') {
      const planTaskRefs = references.filter(value => value.referenceType === 'task').map(value => value.externalId);
      const batch = references.find(value => value.referenceType === 'batch');
      const actions = node('div', undefined, 'actions');
      if (!batch) {
        const submit = button('创建待执行任务批次', async () => {
          if (!hostReady) { message('请从 ChatOS 客户端的“应用”中打开插件后再提交任务。'); return; }
          if (!await ask('创建待执行任务批次？', '宿主会在当前项目中创建 Task DAG，但不会立即运行。创建后还要在任务工作区再次确认。', '创建任务')) return;
          await submitExecution(intent, plan);
        }, true);
        actions.append(submit);
      } else {
        actions.append(button('刷新真实状态', () => refreshExecutionStatuses(intent, true)));
        actions.append(button('打开任务工作区', async () => {
          try { await hostRequest('task.workspace.open', { batchId: batch.externalId, taskIds: planTaskRefs }); }
          catch (error) { message(error.message); }
        }, true));
        refreshExecutionStatuses(intent);
      }
      editor.append(actions);
      if (!hostReady) editor.append(node('p', '任务桥接只在 ChatOS 客户端的受限插件宿主中可用；本地预览不会获得账户凭据。', 'hint'));
    }
    return;
  }
  meta(editor, '准备执行交接', 'prepared');
  const available = state.plans.filter(value => value.status === 'approved' && !state.executionIntents.some(intent => intent.planId === value.id));
  if (!available.length) {
    empty(editor, '没有可交接的规划', '先批准一个尚未创建执行意图的冻结规划版本。', button('查看规划版本', () => switchTab('plans'), true)); return;
  }
  const form = node('form');
  field(form, 'planId', '冻结规划', available[0].id, available.map(value => [value.id, `${value.title} · v${value.version}`]));
  field(form, 'title', '交接标题', `执行 ${available[0].title}`);
  form.append(node('p', '准备后仍需人工确认。插件不会直接调用任务接口，也不会伪造运行状态。', 'hint detail-note'));
  formSave(form, data => ({ operation: 'execution.prepare', planId: data.get('planId'), title: data.get('title') }), '准备执行意图');
  editor.append(form);
}

async function submitExecution(intent, plan) {
  busy = true; availability(); updateStatus(); message();
  try {
    const tasks = plan.workItems.map(item => ({
      clientRef: item.id,
      title: item.title,
      objective: [item.detail, `验收标准：${item.acceptanceCriteria}`].filter(Boolean).join('\n\n'),
      detail: item.detail || null,
      acceptanceCriteria: item.acceptanceCriteria || null,
      prerequisiteRefs: plan.dependencies
        .filter(value => value.kind === 'work_item' && value.id === item.id)
        .map(value => value.prerequisiteId)
    }));
    const result = await hostRequest('task.batch.prepare', {
      idempotencyKey: `${intent.id}:${plan.id}:v${plan.version}`,
      tasks
    });
    busy = false; availability(); updateStatus();
    await change({
      operation: 'execution.link_batch', intentId: intent.id,
      batchId: result.batchID, revision: result.batchID,
      tasks: result.tasks.map(item => ({ clientRef: item.clientRef, taskId: item.taskID }))
    });
    result.tasks.forEach(item => taskStatus.set(item.taskID, item));
  } catch (error) { message(error.message); }
  finally { busy = false; availability(); updateStatus(); }
}

async function refreshExecutionStatuses(intent, announce = false) {
  const ids = state.executionReferences.filter(value => value.intentId === intent.id && value.referenceType === 'task').map(value => value.externalId);
  if (!ids.length || !hostReady || statusLoads.has(intent.id)) return;
  statusLoads.add(intent.id);
  try {
    const values = await hostRequest('task.batch.status', { taskIds: ids });
    values.forEach(item => taskStatus.set(item.taskID, item));
    if (tab === 'executions' && selected === intent.id && !editing) render();
    if (announce) message('已从 Task Runner 刷新真实状态。');
  } catch (error) { if (announce) message(error.message); }
  finally { statusLoads.delete(intent.id); }
}

async function switchTab(next) {
  if (!state || next === tab || !await mayLeave()) return;
  tab = next; creating = false; editing = false; selected = null; $('search').value = ''; message();
  selected = sourceItems()[0]?.id ?? null; render();
}

async function startCreate() {
  if (!state || !await mayLeave()) return;
  creating = true; editing = true; selected = null; $('search').value = ''; message(); render();
  $('editor').querySelector('input[name=title]')?.focus();
}

document.querySelectorAll('[data-icon]').forEach(element => element.append(icon(element.dataset.icon)));
document.querySelectorAll('[data-tab]').forEach(element => { element.onclick = () => switchTab(element.dataset.tab); });
$('create').onclick = startCreate;
$('search').oninput = renderList;
$('editor').addEventListener('input', updateStatus);
$('editor').addEventListener('change', updateStatus);
$('refresh').onclick = async () => {
  if (!await mayLeave()) return;
  busy = true; availability(); $('save-status').textContent = '正在读取…'; documentCache.clear();
  try { await load(); message(); } catch (error) { message(`刷新失败：${error.message}`); }
  finally { busy = false; availability(); updateStatus(); }
};
window.addEventListener('beforeunload', event => {
  if (isDirty() || pending || busy) { event.preventDefault(); event.returnValue = ''; }
});
window.addEventListener('chatos:host-ready', event => {
  hostReady = Array.isArray(event.detail?.capabilities)
    && ['task.batch.prepare', 'task.batch.status', 'task.workspace.open'].every(value => event.detail.capabilities.includes(value));
  if (state) render();
});
window.addEventListener('keydown', event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLocaleLowerCase() === 's') {
    event.preventDefault();
    if (!busy && !pending && !$('confirm-dialog').open) $('editor').querySelector('form')?.requestSubmit();
  }
});

remember();
load().catch(error => {
  empty($('editor'), '无法读取项目资料', '请确认本地插件正在运行，然后刷新重试。');
  message(`读取失败：${error.message}`); remember(); availability(); updateStatus();
});
