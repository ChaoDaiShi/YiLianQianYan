const paths = {
  connectors: '<circle cx="12" cy="12" r="3"/><circle cx="4" cy="4" r="2"/><circle cx="20" cy="4" r="2"/><circle cx="4" cy="20" r="2"/><circle cx="20" cy="20" r="2"/><path d="m6 6 4 4m4 4 4 4m0-12-4 4m-4 4-4 4"/>',
  chat: '<path d="M20 11.5a8 8 0 0 1-8 8H5l-3 2v-10a9 9 0 0 1 18 0Z"/><path d="M7 10h10M7 14h6"/>',
  capability: '<path d="m12 2 5 3v6l-5 3-5-3V5Zm0 6 5-3m-5 3v6m0-6L7 5M7 11l-5 3v6l5 3 5-3v-6m-5 3 5-3m-5 3v6m0-6-5-3m15-3 5 3v6l-5 3-5-3m5-3 5-3m-5 3v6m0-6-5-3"/>',
  more: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  agent: '<rect x="4" y="7" width="16" height="13" rx="5"/><path d="M12 3v4M8 12v2m8-2v2m-7 3h6"/>',
  memory: '<path d="M9 4C5 2 2 6 4 9c-4 3-1 8 2 8 0 4 6 5 6 1V7c0-3-2-4-3-3Zm6 0c4-2 7 2 5 5 4 3 1 8-2 8 0 4-6 5-6 1"/><path d="M5 10h3m8 4h4"/>',
  knowledge: '<path d="M12 5C8 2 3 3 3 3v16s5-1 9 2c4-3 9-2 9-2V3s-5-1-9 2Zm0 0v16"/>',
  folder: '<path d="M3 6h6l2 2h10v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6Zm0 0V4h6l2 2"/>',
  monitor: '<path d="M3 12h4l3-8 4 16 3-8h4"/>',
  logs: '<rect x="5" y="3" width="14" height="18" rx="2"/><path d="M9 7h6m-6 5h6m-6 5h4"/>',
  settings: '<path d="m9 3-1 3-3 1-2 4 2 2v4l4 3 3-1 3 1 4-3v-4l2-2-2-4-3-1-1-3Z"/><circle cx="12" cy="12" r="3"/>',
  shield: '<path d="m12 2 8 4v6c0 5-8 10-8 10S4 17 4 12V6Z"/><path d="m8 12 3 3 5-6"/>',
  model: '<path d="m12 2 9 5v10l-9 5-9-5V7Zm0 10 9-5m-9 5v10m0-10L3 7"/>',
  tool: '<path d="M14 4a6 6 0 0 0-7 8l-5 5 5 5 5-5a6 6 0 0 0 8-7l-4 4-5-5 4-4Z"/>',
  plugin: '<path d="M8 3v5m8-5v5M5 8h14v4a7 7 0 0 1-7 7v3m-7-10a7 7 0 0 0 7 7"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  arrow: '<path d="M5 12h14m-6-6 6 6-6 6"/>',
  down: '<path d="m6 9 6 6 6-6"/>',
  close: '<path d="m6 6 12 12M6 18 18 6"/>',
  mic: '<rect x="9" y="2" width="6" height="13" rx="3"/><path d="M5 10v2a7 7 0 0 0 14 0v-2m-7 9v3m-4 0h8"/>',
  phone: '<path d="m5 3 4 4-2 3c1 3 4 6 7 7l3-2 4 4-2 3C9 23 1 15 2 5Z"/>',
  audio: '<path d="M4 9h4l5-5v16l-5-5H4Zm13-2c3 2 3 8 0 10m3-13c4 4 4 12 0 16"/>',
  attach: '<path d="m8 12 7-7c5-5 11 1 6 6L10 22C3 28-4 19 2 13L13 2m-8 14 10-10"/>',
  send: '<path d="m3 3 18 9-18 9 4-9Zm4 9h14"/>',
  sparkle: '<path d="m12 2 3 7 7 3-7 3-3 7-3-7-7-3 7-3Z"/>',
  workflow: '<rect x="2" y="9" width="6" height="6" rx="1"/><rect x="16" y="2" width="6" height="6" rx="1"/><rect x="16" y="16" width="6" height="6" rx="1"/><path d="M8 12h4V5h4m-4 7v7h4"/>',
  globe: '<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
  image: '<rect x="3" y="3" width="18" height="18" rx="3"/><circle cx="8" cy="8" r="1"/><path d="m3 18 6-6 4 4 3-3 5 5"/>',
  expand: '<path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5"/>',
  minus: '<path d="M5 12h14"/>',
  import: '<path d="M12 2v12m-4-4 4 4 4-4M4 15v6h16v-6"/>',
};
const icon = name => `<svg class="ico" viewBox="0 0 24 24" aria-hidden="true">${paths[name] || paths.sparkle}</svg>`;
const orb = () => `<div class="orb">${icon('sparkle')}</div>`;
const wave = () => `<div class="wave">${[7,14,25,16,32,22,13,27,35,19,10,21,29,14,7].map(h=>`<i style="height:${h}px"></i>`).join('')}</div>`;
const views = [['connectors','连接器'],['more','更多导航'],['call','自然通话'],['capabilities','能力导航'],['settings','模型池'],['pluginfeature','插件功能'],['security','安全控制'],['monitor','Token 监控']];
let view = new URLSearchParams(location.search).get('view') || 'connectors';
let dark = new URLSearchParams(location.search).get('theme') === 'dark';
let boards=[{id:'canvas-01',name:'日常工作',mode:'all'},{id:'canvas-02',name:'项目演示',mode:'all'},{id:'canvas-03',name:'学习资料',mode:'conversations'}];
let activeBoard='canvas-01';
let historyBoards=[{id:'TG-19E8B882',name:'项目演示任务图',running:false},{id:'TG-33123E7A',name:'资料整理任务图',running:false},{id:'TG-58096B5A',name:'文档处理任务图',running:true}];
let managerKind='connectors';
const nodeTitles={};
const boardDisplay={};
const nav = [['connectors','连接器'],['chat','对话'],['folder','工作空间'],['capability','能力']];
const secondaryNav = [['agent','智能体'],['memory','记忆'],['knowledge','知识库'],['monitor','监控'],['logs','日志'],['plugin','插件功能']];
function navItem([id,label],active) { return `<button class="navitem ${active===id?'active':''}" data-nav="${id}" title="${label}">${icon(id)}<span>${label}</span>${active===id?'<i class="navstar">✦</i>':''}</button>`; }
function shell(content,active,title) {
  const template=document.createElement('template');template.innerHTML=content;
  const headingElement=template.content.querySelector('.heading');
  const pageTitle=headingElement?.querySelector('h1')?.textContent || title;
  const description=headingElement?.querySelector('p')?.textContent || '与小昔涟一起，把想法变成行动';
  const actions=headingElement ? headingElement.querySelector('.actions').innerHTML : '<span class="pill rose">小涟 · 日常助手</span>';
  headingElement?.remove();
  const secondaryActive=active==='more'||secondaryNav.some(([id])=>id===active);
  return `<aside class="sidebar"><button class="brand-avatar" data-nav="chat" title="忆涟千言 · 新对话"><img src="assets/favicon.png" alt="忆涟千言"></button><div class="rail-divider"></div>
  <div class="rail-primary">${nav.map(n=>navItem(n,active)).join('')}<button class="navitem ${secondaryActive?'active':''}" data-view="more" title="更多功能导航">${icon('more')}<span>更多</span>${secondaryActive?'<i class="navstar">✦</i>':''}</button></div>
  <div class="sidebar-bottom"><button class="rail-action ${view==='security'?'active':''}" data-view="security" title="安全与审批">${icon('shield')}</button><button class="rail-action ${view==='settings'?'active':''}" data-view="settings" title="设置">${icon('settings')}</button><div class="rail-status" title="后端正常"><i class="dot"></i></div></div></aside>
  <main class="main"><header class="topbar"><div class="page-title-group">${secondaryActive&&active!=='more'?'<button class="textbtn" data-view="more">← 更多</button>':''}<h1>${pageTitle}</h1><p>${description}</p></div><div class="actions">${actions}</div></header>${template.innerHTML}</main>`;
}
function heading(en,title,desc,actions='') { return `<div class="heading"><div><div class="eyebrow">${en}</div><h1>${title}</h1><p>${desc}</p></div><div class="actions">${actions}</div></div>`; }
function node(name,title,desc,cls,extra='') { const key=name==='chat'?`chat-${cls.split(' ')[1]}`:name;return `<div class="node ${cls}" data-node="${key}" role="button" tabindex="0" aria-label="编辑${title}"><div class="node-top">${icon(name)}<h3>${nodeTitles[key]||title}</h3>${extra}</div><p>${desc}</p></div>`; }
function connector() {
  const board=boards.find(b=>b.id===activeBoard),mode=board?.mode||'all';
  const content=`<section class="page">${heading('CONNECTIONS','连接器','点击节点即可修改，按需要组织你的工作画布。',`<select data-board-choice aria-label="切换画布">${boards.map(b=>`<option value="${b.id}" ${b.id===activeBoard?'selected':''}>${b.name}</option>`).join('')}</select><button data-manage>管理画布</button><button class="primary" data-add-connection>${icon('plus')} 添加连接</button>`)}
  ${!board?'<div class="canvas empty-canvas"><h2>还没有画布</h2><p>创建一张画布，连接对话与能力。</p><button class="primary" data-create-board>新建画布</button></div>':`<div class="canvas" data-mode="${mode}"><div class="canvas-topbar"><div class="segmented">${[['all','全部'],['conversations','仅对话'],['capabilities','仅能力']].map(([id,label])=>`<button data-filter="${id}" class="${mode===id?'selected':''}">${label}</button>`).join('')}</div><span class="pill">小涟 · 日常助手</span><button data-display>${icon('settings')} 显示设置</button></div><div class="canvas-world">
    <svg class="wires" viewBox="0 0 960 570" preserveAspectRatio="none"></svg>
    <span class="canvas-label left">对话 / CONVERSATIONS</span><span class="canvas-label right">能力与资源 / RESOURCES</span><span class="canvas-label bottom">上下文 / CONTEXT</span>
    <div class="node safety" data-node="shield" role="button" tabindex="0" aria-label="编辑安全约束"><div class="node-top">${icon('shield')}<h3>安全约束</h3><span class="pill green">已生效</span></div><div class="safety-tags"><span class="pill green">工作区 · 完全权限</span><span class="pill">系统操作 · 需批准</span></div></div>
    ${node('chat','准备项目演示','今天 14:32 · 演示结构与讲稿','conv one')}
    ${node('chat','梳理产品交互','今天 15:08 · 当前对话','conv two','<span class="pill rose">当前</span>')}
    ${node('chat','整理学习资料','昨天 · 文档与知识归档','conv three')}
    <div class="node agent" data-node="agent" role="button" tabindex="0" aria-label="编辑智能体">${orb()}<h2>${nodeTitles.agent||'小涟'}</h2><div class="agent-meta">日常助手 · 温柔、清晰、有条理</div><footer><span class="pill green">可对话</span><span class="pill">角色已启用</span></footer></div>
    ${node('model','模型池','日常对话池 · 3 级候选','resource r1','<span class="pill">3 级</span>')}
    ${node('capability','能力','Skill · 工作流 · MCP','resource r2','<span class="pill">6</span>')}
    ${node('tool','工具','文件读写 · 桌面操作','resource r3','<span class="pill">8</span>')}
    ${node('plugin','插件','浏览器 · 本地服务','resource r4','<span class="pill">2</span>')}
    ${node('memory','记忆','偏好与长期上下文','context mem')}
    ${node('knowledge','知识库','项目文档与学习资料','context kb')}
    </div>
    <div class="canvas-tools"><button aria-label="缩小">${icon('minus')}</button><span>100%</span><button aria-label="放大">${icon('plus')}</button><button aria-label="适应视图">${icon('expand')}</button><span>密集布局</span></div><div class="canvas-caption">点击节点编辑 · 隐藏不影响绑定 · ${board.name}</div>
  </div>`}</section>`;
  return shell(content,'connectors','连接器');
}
function morePage(){
  const entries=[['agent','智能体','设定角色、提示词与协作方式'],['memory','记忆','管理小涟记住的偏好与事实'],['knowledge','知识库','组织文档与知识来源'],['monitor','监控','查看运行状态与 Token 消耗'],['logs','日志','追踪运行记录与操作过程'],['settings','账号与偏好','管理账号、外观与个人设置'],['plugin','文档工作台','文档助手插件 · 整理、汇总与导出']];
  return shell(`<section class="page">${heading('NAVIGATION','更多','选择要进入的功能。')}<div class="navigation-intro"><span class="eyebrow">你的工作台</span><h2>需要的功能，都在这里</h2><p>常用功能与插件新增入口，集中访问。</p></div><div class="navigation-grid">${entries.map(([id,label,desc])=>`<button class="navigation-card panel ${id==='plugin'?'plugin-feature-tile':''}" data-destination="${id}"><span class="nav-card-icon">${icon(id)}</span><h2>${label}${id==='plugin'?'<span class="pill rose">插件功能</span>':''}</h2><p>${desc}</p><span class="card-go">进入 ${icon('arrow')}</span></button>`).join('')}</div></section>`,'more','更多');
}
function call() {
  const content=`<div class="chat-area"><aside class="conversation-list"><button class="primary">${icon('plus')} 新对话</button><div class="detail-label">最近的对话</div><div class="convitem selected">梳理产品交互<small>刚刚 · 小涟</small></div><div class="convitem">准备项目演示<small>今天 14:32</small></div><div class="convitem">整理学习资料<small>昨天</small></div></aside>
  <section class="chat-body"><div class="chat-title">梳理产品交互 <span class="pill" style="float:right">小涟 · 日常助手</span></div>
  <div class="chat-messages"><div class="message-user">小涟，帮我把今天的想法整理一下。</div><div class="message-agent">${orb()}<div class="message-copy"><strong>小涟 <span class="pill rose">语音通话中</span></strong><p>好呀，我们可以边聊边整理。<br>先从你最想改变的地方开始，我会把要点记下来。</p><div class="panel"><p>已经记下的方向</p><p style="margin-top:10px">01　让对话和能力的关系更清楚<br>02　把配置收进简洁的弹窗<br>03　让语音交流像一次自然的通话</p></div></div></div><div class="message-user">对，页面切换的时候也不要打断通话。</div><div class="message-agent">${orb()}<div class="message-copy"><strong>小涟</strong><p>我在听。你可以继续操作页面，<br>也可以把这个小窗口收成一个圆。</p></div></div></div>
  <div class="composer"><div class="composer-placeholder">告诉小昔涟你想完成什么……</div><div class="composer-footer"><button class="iconbtn" title="添加附件">${icon('attach')}</button><button class="iconbtn" title="语音输入">${icon('mic')}</button><button class="iconbtn call-icon" title="与小涟通话">${icon('phone')}</button><span class="hint">Enter 发送 · Shift + Enter 换行</span><button class="iconbtn send" title="发送">${icon('send')}</button></div></div>
  </section></div>
  <aside class="call-panel"><div class="call-top"><span><i class="dot"></i>通话中 · 02:18</span><button class="iconbtn" title="收起" data-collapse>${icon('minus')}</button></div>${orb()}<h2>小涟</h2><p>正在听你说 · 梳理产品交互</p>${wave()}<p>“页面切换的时候也不要打断通话。”</p><div class="call-controls"><button title="静音" data-mute>${icon('mic')}</button><button title="扬声器">${icon('audio')}</button><button title="挂断" class="end">${icon('phone')}</button></div></aside>
  <div class="collapsed-call"><button title="展开通话" data-expand>${orb()}</button><small>收起形态</small></div>`;
  return shell(content,'chat','对话');
}
const capRows = [
 ['sparkle','文档整理','将零散内容整理为清晰文档','本地'],
 ['knowledge','资料阅读','阅读资料并提取关键内容','已导入'],
 ['sparkle','演示讲稿','整理演示结构并撰写讲稿','本地'],
 ['folder','工作区整理','按规则组织目录与文件','本地'],
 ['image','图像理解','理解截图、图表与界面','已导入'],
 ['globe','信息研究','通过 AI 搜索整理信息来源','本地'],
 ['logs','会议纪要','记录要点与后续行动','已导入'],
];
function capRow(item,i) { return `<div class="list-row ${i===0?'chosen':''}"><span class="list-icon">${icon(item[0])}</span><div class="grow"><h3>${item[1]}</h3><p>${item[2]}</p></div><span class="pill">${item[3]}</span></div>`; }
function capability() {
  const cats=[['sparkle','Skill','可复用的工作方法'],['workflow','工作流','串联步骤与自动化'],['plugin','MCP','连接外部工具与资源'],['tool','工具','查看原子操作能力'],['capability','插件','扩展小涟的能力']];
  return shell(`<section class="page">${heading('CAPABILITIES','能力','选择能力类别，点击卡片进行管理。')}<div class="navigation-intro"><span class="eyebrow">CAPABILITIES</span><h2>让小涟拥有合适的能力</h2><p>从 Skill 到本地服务，以你需要的方式连接。</p></div><div class="navigation-grid">${cats.map(c=>`<button class="navigation-card panel" data-open-cap="${c[1]}"><span class="nav-card-icon">${icon(c[0])}</span><h2>${c[1]}</h2><p>${c[2]}</p><span class="card-go">管理 ${icon('arrow')}</span></button>`).join('')}<button class="navigation-card panel import-tile" data-import><span class="nav-card-icon">${icon('import')}</span><h2>导入与本地发现</h2><p>导入外部能力，发现本地运行的服务</p><span class="card-go">添加能力 ${icon('arrow')}</span></button></div></section>
  <div class="dialog-overlay" id="primary-dialog"><section class="dialog"><header class="dialog-header"><div><h2>能力库</h2><p>选择能力查看说明，或连接到你的智能体。</p></div><button class="iconbtn" data-close title="关闭">${icon('close')}</button></header><div class="tabs">${cats.map((c,i)=>`<button class="${i===0?'selected':''}" data-cap-tab>${c[1]}</button>`).join('')}</div><div class="dialog-body"><div class="cap-manager"><div class="cap-list scrollbar">${capRows.map(capRow).join('')}</div><div class="cap-detail"><span class="list-icon">${icon('sparkle')}</span><h2>文档整理</h2><div class="detail-chips"><span class="pill green">已启用</span><span class="pill">本地 Skill</span></div><p>把讨论、笔记和资料整理成结构清楚的文档，保留关键信息和来源。</p><button class="textbtn" data-more>查看更多 →</button><div class="detail-label">已连接的智能体</div><div class="detail-chips"><span class="pill rose">小涟 · 日常助手</span></div><div class="detail-label">来源</div><p style="margin-top:8px">我的能力 / document-organizer</p><div class="detail-label">使用的工具</div><div class="detail-chips"><span class="pill">读取文件</span><span class="pill">写入文件</span></div></div></div></div><footer class="dialog-footer"><span>7 项 Skill · 列表独立滚动</span><div class="actions"><button>${icon('import')} 导入</button><button class="primary">连接到智能体 ${icon('arrow')}</button></div></footer></section></div>`,'capability','能力');
}
const serviceItems=[['model','对话模型','DeepSeek · 日常对话'],['mic','语音识别','MiniMax · 语音输入'],['audio','语音合成','MiniMax · 小涟声音'],['globe','搜索','AI 的资料检索服务'],['image','识图','截图与图像理解'],['sparkle','生图','图像生成服务'],['knowledge','Embedding','知识检索向量模型']];
function settings() {
  const providers=[['O','OpenAI'],['D','DeepSeek'],['Q','阿里云百炼'],['G','智谱 GLM'],['C','Anthropic Claude'],['G','Google Gemini'],['M','MiniMax'],['K','Moonshot Kimi'],['豆','火山方舟 / 豆包'],['+','更多厂商']];
  return shell(`<section class="page">${heading('PREFERENCES','设置','按你的习惯，让小涟更顺手。')}<div class="settings-layout"><aside class="settings-nav">${['基础配置','安全控制','智能体偏好','能力目录','外观'].map((x,i)=>`<button class="${i===0?'selected':''}">${x}</button>`).join('')}</aside><div class="settings-content"><div class="service-grid">${serviceItems.map(x=>`<button class="service-card panel" data-open-profile>${icon(x[0])}<h3>${x[1]}</h3><p>${x[2]}</p></button>`).join('')}</div></div></div></section>
  <div class="dialog-overlay" id="primary-dialog"><section class="dialog"><header class="dialog-header"><div><h2>对话模型</h2><p>选择官方厂商，配置模型和访问凭据。</p></div><button class="iconbtn" data-close title="关闭">${icon('close')}</button></header><div class="dialog-body"><div class="profile-body"><aside class="provider-list scrollbar"><div class="detail-label">官方厂商</div>${providers.map((p,i)=>`<div class="provider-row ${i===1?'selected':''}"><span class="provider-logo">${p[0]}</span>${p[1]}${i===1?icon('check'):''}</div>`).join('')}</aside><div class="form"><div class="form-head"><h3>DeepSeek <span class="pill">官方 API</span></h3><button class="textbtn">查看官方文档 ↗</button></div><div class="form-grid"><div class="field"><label>档案名称</label><div class="input">日常对话</div></div><div class="field"><label>服务区域</label><div class="input">默认区域 ${icon('down')}</div></div></div><div class="field"><label>API 地址</label><div class="input">https://api.deepseek.com/v1</div></div><div class="form-grid"><div class="field"><label>模型</label><div class="input">deepseek-chat ${icon('down')}</div></div><div class="field"><label>访问密钥</label><div class="input">•••••••••••••••• <span class="muted">已保存</span></div></div></div><div class="actions" style="margin:0 0 20px"><button class="textbtn">高级参数 →</button><span class="muted small" style="margin-left:auto">设为默认对话模型 <span style="color:var(--green)">●</span></span></div><div class="connection-result">${icon('check')} 连接成功 <span style="float:right;font-size:10px">刚刚完成测试</span></div></div></div></div><footer class="dialog-footer"><span>凭据统一保存在本机</span><div class="actions"><button>测试连接</button><button class="primary">保存配置</button></div></footer></section></div>`,'settings','设置 / 基础配置');
}
function security() {
  const rows=[['folder','工作区文件','当前工作区目录','full','完全权限'],['tool','命令与进程','系统命令与进程操作','approve','需批准'],['globe','网络访问','已连接的服务地址','full','完全权限'],['monitor','桌面控制','鼠标、键盘与窗口','approve','需批准'],['plugin','MCP 与插件','新连接的外部工具','approve','需批准'],['shield','系统目录写入','Windows 系统目录','deny','禁止'],['memory','记忆与知识','当前智能体绑定的资料','full','完全权限']];
  return shell(`<section class="page">${heading('ACCESS CONTROL','安全控制','每一项权限，都对应小涟实际可以做的事。',`<button>小涟 · 日常助手 ${icon('down')}</button>`)}<div class="panel policy-banner"><div><h3>默认权限</h3><p>具体能力可以设置独立规则。匹配的禁止规则优先。</p></div><div class="segmented"><button>完全权限</button><button class="selected">需批准</button><button>禁止</button></div></div><div class="section-title"><h3>能力与资源权限</h3><button class="textbtn">${icon('plus')} 添加规则</button></div><div class="panel"><table class="policy-table"><thead><tr><th>操作类别</th><th>作用范围</th><th>权限</th><th>规则管理</th></tr></thead><tbody>${rows.map(r=>`<tr><td><div class="permission-name">${icon(r[0])}${r[1]}</div></td><td class="muted">${r[2]}</td><td><span class="mode-chip ${r[3]}">${r[4]} ${icon('down')}</span></td><td><button class="textbtn">编辑范围 →</button></td></tr>`).join('')}</tbody></table></div><div class="panel policy-note">${icon('shield')}<p><strong>完全权限</strong>范围内直接执行；<strong>需批准</strong>在动作前确认；<strong>禁止</strong>的操作不会执行。</p></div></section>`,'settings','设置 / 安全控制');
}
function chart() {
  return `<svg class="chart" viewBox="0 0 690 205"><defs><linearGradient id="area" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ea91b9" stop-opacity=".25"/><stop offset="1" stop-color="#ea91b9" stop-opacity=".01"/></linearGradient></defs>${[35,80,125,170].map((y,i)=>`<line x1="36" y1="${y}" x2="670" y2="${y}"/><text x="0" y="${y+3}">${[30,20,10,0][i]}k</text>`).join('')}<path d="M42 144C83 144 105 101 147 106S210 148 252 115S317 49 358 68S422 116 463 91S527 24 569 38S627 59 670 42V170H42Z" fill="url(#area)"/><path d="M42 144C83 144 105 101 147 106S210 148 252 115S317 49 358 68S422 116 463 91S527 24 569 38S627 59 670 42" fill="none" stroke="#d483af" stroke-width="2.5"/><path d="M42 160C95 159 109 148 147 150S211 163 252 149S319 129 358 135S422 152 463 142S527 119 569 125S628 137 670 127" fill="none" stroke="#ab94d4" stroke-width="2"/>${['10/14','10/15','10/16','10/17','10/18','10/19','10/20'].map((d,i)=>`<text x="${34+i*104}" y="197">${d}</text>`).join('')}</svg>`;
}
function monitor() {
  return shell(`<section class="page">${heading('OBSERVABILITY','监控','了解小涟的运行状态，以及每一次对话的用量。',`<button>近 7 天 ${icon('down')}</button><button>全部模型 ${icon('down')}</button><button class="primary">${icon('expand')} 桌面浮窗</button>`)}<div class="metric-grid">${[['输入 Token','128,460','prompt'],['输出 Token','32,180','completion'],['总 Token','160,640','total']].map((m,i)=>`<div class="panel metric"><div class="label">${m[0]}${icon(i===2?'monitor':'model')}</div><div class="number">${m[1]}<small>${m[2]}</small></div></div>`).join('')}</div><div class="monitor-grid"><section class="panel chart-panel"><div class="chart-head"><h3>Token 趋势</h3><span><i class="dot" style="background:#d483af"></i>输入　<i class="dot" style="background:#ab94d4"></i>输出</span></div>${chart()}</section><section class="panel system-panel"><h3>系统资源</h3><div class="resource-meter"><div><span>CPU</span><span>18%</span></div><div class="meter-track"><i style="width:18%"></i></div></div><div class="resource-meter"><div><span>内存</span><span>42%</span></div><div class="meter-track"><i style="width:42%;background:var(--rose)"></i></div></div><div class="resource-meter"><div><span>小涟状态</span><span style="color:var(--green)">待命中</span></div></div></section></div><div class="section-title"><h3>按模型查看</h3><span class="small muted">时区 · Asia/Shanghai</span></div><div class="panel" style="max-width:740px"><div class="list-row"><span class="list-icon">${icon('model')}</span><div class="grow"><h3>日常对话 · DeepSeek</h3><p>输入 96,380 · 输出 24,120</p></div><strong style="font-size:15px">120,500</strong></div><div class="list-row"><span class="list-icon">${icon('model')}</span><div class="grow"><h3>写作助手 · 通义千问</h3><p>输入 32,080 · 输出 8,060</p></div><strong style="font-size:15px">40,140</strong></div></div></section><div class="monitor-ball">${icon('monitor')}</div><aside class="monitor-float"><header><span><i class="dot"></i>小涟 · 运行概览</span>${icon('minus')}</header><div class="number">160,640</div><small>近 7 天 Token</small><div class="float-meters"><span>CPU　18%</span><span>内存　42%</span></div></aside>`,'monitor','监控');
}
function escapeText(value){return String(value).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;');}
function openEditor(title,body,footer='<button class="primary" data-editor-save>保存修改</button>'){
  const layer=document.createElement('div');layer.className='dialog-overlay editor-layer';layer.innerHTML=`<section class="dialog editing-dialog" role="dialog" aria-label="${title}"><header class="dialog-header"><div><h2>${title}</h2><p>修改后应用到对应对象，画布位置保持不变。</p></div><button class="iconbtn" data-editor-close title="关闭">${icon('close')}</button></header><div class="dialog-body">${body}</div><footer class="dialog-footer"><span>当前画布 · ${escapeText(boards.find(b=>b.id===activeBoard)?.name||'工作台')}</span><div class="actions"><button data-editor-cancel>取消</button>${footer}</div></footer></section>`;
  document.querySelector('.main').appendChild(layer);layer.querySelector('[data-editor-close]').onclick=()=>layer.remove();layer.querySelector('[data-editor-cancel]').onclick=()=>layer.remove();return layer;
}
const editDefinitions={
  agent:['智能体与角色','智能体名称','小涟','用户提示词','用温柔、清晰、有条理的方式与我交流。'],
  shield:['安全约束','规则名称','日常工作权限','权限范围','当前工作区'],
  model:['模型池与绑定','节点名称','模型池','当前对话模型池','日常对话池 · DeepSeek → 通义千问 → OpenAI'],
  capability:['能力与绑定','节点名称','能力','已连接的能力','文档整理、项目工作流、本地 MCP'],
  tool:['工具与权限','节点名称','工具','允许使用的工具','读取文件、写入文件、桌面操作'],
  plugin:['插件管理','节点名称','插件','服务连接','浏览器、本地服务'],
  memory:['记忆与上下文','节点名称','记忆','记忆范围','个人偏好、项目约定'],
  knowledge:['知识库绑定','节点名称','知识库','已绑定知识库','项目文档、学习资料'],
};
function editNode(key){
  const isChat=key.startsWith('chat-'),definition=isChat?['对话设置','对话标题',document.querySelector(`[data-node="${key}"] h3`).textContent,'关联智能体','小涟 · 日常助手']:editDefinitions[key];
  const [title,label,initial,secondLabel,secondValue]=definition;
  const content=`<div class="editor-context">${icon(isChat?'chat':key)}<div><strong>${title}</strong><p>从连接器直接编辑，无需离开画布。</p></div></div><div class="field"><label>${label}</label><input class="input" id="node-title" value="${escapeText(nodeTitles[key]||initial)}"></div><div class="field"><label>${secondLabel}</label>${key==='agent'?`<textarea class="input" rows="4">${secondValue}</textarea>`:`<select class="input"><option>${secondValue}</option><option>${key==='model'?'写作助手 · 通义千问':'当前智能体专属配置'}</option></select>`}</div>${key==='model'?'<div class="editor-notice">当前操作修改小涟的模型绑定。<button class="textbtn">编辑共享模型档案 →</button></div>':key==='shield'||key==='tool'?'<div class="field"><label>执行权限</label><div class="segmented"><button>完全权限</button><button class="selected">需批准</button><button>禁止</button></div></div>':'<div class="editor-notice">保存后更新节点摘要；取消不更改原有内容。</div>'}<div class="editor-secondary"><button class="textbtn" data-hide-node>从此画布移除</button><button class="textbtn" data-unbind>解除连接…</button></div>`;
  const layer=openEditor(title,content);
  layer.querySelector('[data-editor-save]').onclick=()=>{nodeTitles[key]=escapeText(layer.querySelector('#node-title').value);layer.remove();render();};
  layer.querySelector('[data-hide-node]').onclick=()=>{if(key==='agent')return;const config=boardDisplay[activeBoard]||{};config.hidden=[...(config.hidden||[]),key];boardDisplay[activeBoard]=config;layer.remove();render();};
  if(key==='agent')layer.querySelector('[data-hide-node]').disabled=true;
  layer.querySelector('[data-unbind]').onclick=()=>{const nested=openEditor('解除连接',`<p>解除 ${title} 与当前智能体的绑定。原始对象仍保留。</p>`,'<button class="primary" data-editor-save>解除连接</button>');nested.querySelector('[data-editor-save]').onclick=()=>{const config=boardDisplay[activeBoard]||{};config.hidden=[...(config.hidden||[]),key];boardDisplay[activeBoard]=config;render();};};
}
function displayEditor(){
  const config=boardDisplay[activeBoard]||{},kinds=[['chat','对话'],['shield','安全约束'],['model','模型'],['capability','能力'],['tool','工具'],['plugin','插件'],['memory','记忆'],['knowledge','知识库']];
  const layer=openEditor('显示设置',`<p class="display-explain">选择这张画布显示的内容。隐藏节点不改变实际绑定。</p><div class="display-options">${kinds.map(([key,label])=>`<label><input type="checkbox" name="visible-kind" value="${key}" ${!config.kinds||config.kinds.includes(key)?'checked':''}>${icon(key)}${label}</label>`).join('')}</div><div class="field"><label>布局密度</label><select class="input" id="density"><option value="compact">密集 · 节点边缘间距 56px</option><option value="comfortable" ${config.density==='comfortable'?'selected':''}>舒展 · 节点边缘间距 88px</option></select></div><div class="display-toggles"><label><input type="checkbox" id="show-edges" ${config.edges===false?'':'checked'}> 显示连线</label><label><input type="checkbox" id="show-labels" ${config.labels===false?'':'checked'}> 显示分区标签</label></div>`);
  layer.querySelector('[data-editor-save]').onclick=()=>{boardDisplay[activeBoard]={...config,kinds:[...layer.querySelectorAll('[name="visible-kind"]:checked')].map(x=>x.value),density:layer.querySelector('#density').value,edges:layer.querySelector('#show-edges').checked,labels:layer.querySelector('#show-labels').checked};boards.find(b=>b.id===activeBoard).mode='custom';render();};
}
function canvasManager(){
  const rows=managerKind==='connectors'?boards:historyBoards;
  const layer=openEditor('管理画布',`<div class="tabs"><button data-manager-kind="connectors" class="${managerKind==='connectors'?'selected':''}">连接器画布</button><button data-manager-kind="history" class="${managerKind==='history'?'selected':''}">历史任务画布</button></div><p class="manager-summary">共 ${rows.length} 张画布 · 每一张都可以单独管理和删除</p><div class="canvas-list">${rows.map(b=>`<div class="canvas-row" data-board-row="${b.id}"><span class="list-icon">${icon('connectors')}</span><div class="grow"><h3>${escapeText(b.name)} ${b.id===activeBoard?'<span class="pill rose">当前</span>':''}</h3><p>${b.id} · ${managerKind==='connectors'?'连接器画布':b.running?'运行中':'历史任务画布'}</p></div>${managerKind==='connectors'?`<button data-open-board="${b.id}">打开</button>`:''}<button class="delete-link" data-delete-board="${b.id}">删除</button></div>`).join('')}</div><div class="editor-notice">删除连接器画布保留智能体及实际绑定。历史任务画布删除不影响外部成果文件。</div>`,'<button class="primary" data-create-board>新建画布</button>');
  layer.classList.add('manager-layer');layer.querySelector('.dialog-header p').textContent='按画布 ID 操作，删除非当前画布不会切走当前页面。';
  layer.querySelectorAll('[data-manager-kind]').forEach(b=>b.onclick=()=>{managerKind=b.dataset.managerKind;layer.remove();canvasManager();});
  layer.querySelectorAll('[data-delete-board]').forEach(b=>b.onclick=()=>deleteBoardConfirmation(b.dataset.deleteBoard,managerKind));
  layer.querySelectorAll('[data-open-board]').forEach(b=>b.onclick=()=>{activeBoard=b.dataset.openBoard;render();});
  layer.querySelector('[data-create-board]').onclick=createBoard;
}
function deleteBoardConfirmation(id,kind){
  const list=kind==='connectors'?boards:historyBoards,target=list.find(b=>b.id===id);
  const layer=openEditor('删除画布',`<div class="delete-target"><span class="list-icon">${icon('connectors')}</span><div><h2>${escapeText(target.name)}</h2><p>${target.id}</p></div></div><p>${kind==='connectors'?'将删除这张画布及其显示设置。智能体、对话、资源和实际绑定继续保留。':'将删除该任务图及专属画布数据，保留外部成果文件。'}</p><div class="editor-notice">${id===activeBoard?'这是当前画布，删除后切换到另一张；没有其他画布时显示创建入口。':target.running?'该图正在运行。先停止运行，结束后再删除。':'这不是当前画布，删除后你正在查看的画布保持不变。'}</div>` ,`<button class="danger-button" data-confirm-delete>${target.running?'停止并删除':'删除画布'}</button>`);
  layer.classList.add('delete-layer');layer.querySelector('.dialog-header p').textContent='请核对名称与 ID。';
  layer.querySelector('[data-confirm-delete]').onclick=()=>{const index=list.findIndex(b=>b.id===id);list.splice(index,1);if(kind==='connectors'&&id===activeBoard)activeBoard=list[index]?.id||list[index-1]?.id||'';render();canvasManager();};
}
function createBoard(){const id=`canvas-${Date.now()}`;boards.push({id,name:'新画布',mode:'all'});activeBoard=id;render();}
function updateCanvas(){
  const canvas=document.querySelector('.canvas-world');if(!canvas)return;
  const mode=boards.find(b=>b.id===activeBoard).mode,config=boardDisplay[activeBoard]||{};
  const categories=mode==='conversations'?['chat','agent']:mode==='capabilities'?['agent','capability','model','tool','plugin']:mode==='custom'?['agent',...(config.kinds||[])]:null;
  canvas.querySelectorAll('.node').forEach(n=>{const category=n.dataset.node.startsWith('chat')?'chat':n.dataset.node;n.hidden=category!=='agent'&&((categories&&!categories.includes(category))||(config.hidden||[]).includes(n.dataset.node));});
  canvas.querySelectorAll('.canvas-label').forEach(n=>n.hidden=config.labels===false||mode!=='all');
  canvas.classList.toggle('comfortable',config.density==='comfortable');
  const svg=canvas.querySelector('.wires'),root=canvas.getBoundingClientRect(),agent=canvas.querySelector('.agent').getBoundingClientRect();
  let wires='';const center={x:agent.left-root.left,y:agent.top-root.top,w:agent.width,h:agent.height};
  [...canvas.querySelectorAll('.node:not(.agent)')].filter(n=>!n.hidden).forEach(n=>{
    const r=n.getBoundingClientRect(),p={x:r.left-root.left,y:r.top-root.top,w:r.width,h:r.height};let a,b;
    if(n.classList.contains('safety')){a={x:p.x+p.w/2,y:p.y+p.h};b={x:center.x+center.w/2,y:center.y};}
    else if(n.classList.contains('context')){a={x:center.x+center.w/2,y:center.y+center.h};b={x:p.x+p.w/2,y:p.y};}
    else if(n.classList.contains('conv')){a={x:p.x+p.w,y:p.y+p.h/2};b={x:center.x,y:Math.max(center.y+35,Math.min(center.y+center.h-35,p.y+p.h/2))};}
    else{a={x:center.x+center.w,y:Math.max(center.y+28,Math.min(center.y+center.h-28,p.y+p.h/2))};b={x:p.x,y:p.y+p.h/2};}
    const vertical=n.classList.contains('safety')||n.classList.contains('context');const mid=vertical?(a.y+b.y)/2:(a.x+b.x)/2;wires+=`<path class="${n.classList.contains('conv')?'rose':''}" d="M${a.x} ${a.y} C${vertical?a.x:mid} ${vertical?mid:a.y},${vertical?b.x:mid} ${vertical?mid:b.y},${b.x} ${b.y}"/>`;
  });svg.innerHTML=config.edges===false?'':wires;
}
let destination='agent';
function destinationPage(){const labels={agent:'智能体',memory:'记忆',knowledge:'知识库',logs:'日志'};const label=labels[destination];return shell(`<section class="page">${heading('',label,'管理与当前工作相关的内容。')}<div class="panel list-row"><span class="list-icon">${icon(destination)}</span><div class="grow"><h3>${label==='智能体'?'小涟 · 日常助手':label==='记忆'?'个人偏好与项目约定':label==='知识库'?'项目文档与学习资料':'最近操作记录'}</h3><p>在这里查看与管理对应内容</p></div><button data-destination-edit>查看详情</button></div></section>`,destination,label);}
function selectCapabilityCategory(label){
  const groups={Skill:capRows,工作流:[['workflow','演示准备流程','整理大纲、资料与演示讲稿','可运行'],['workflow','资料归档流程','读取、分类并归档文档','本地']],MCP:[['plugin','本地文件服务','访问授权目录中的文件','已连接'],['globe','浏览器服务','读取页面与操作浏览器','已连接']],工具:[['tool','读取文件','读取指定文件内容','可用'],['tool','写入文件','写入授权目录中的文件','需批准']],插件:[['plugin','浏览器插件','网页操作与页面内容读取','已启用'],['plugin','本地服务插件','接入本机运行的服务','已启用']]};
  const rows=groups[label];document.querySelectorAll('[data-cap-tab]').forEach(t=>t.classList.toggle('selected',t.textContent===label));document.querySelector('.cap-list').innerHTML=rows.map(capRow).join('');document.querySelector('.cap-detail h2').textContent=rows[0][1];document.querySelector('.cap-detail > p').textContent=rows[0][2];document.querySelector('.dialog-footer > span').textContent=`${rows.length} 项 ${label} · 列表独立滚动`;
}
function render() {
  const renderers={connectors:connector,more:morePage,destination:destinationPage,call,capabilities:capability,settings:modelPoolSettings,pluginfeature:pluginFeaturePage,security,monitor};
  document.getElementById('app').className=`app${dark?' dark':''}`;
  document.getElementById('app').innerHTML=renderers[view]();
  document.getElementById('review-nav').innerHTML=views.map(([id,label])=>`<button class="${view===id?'active':''}" data-view="${id}">${label}</button>`).join('');
  document.getElementById('theme-toggle').textContent=dark?'切换浅色':'切换深色';
  document.querySelectorAll('[data-view]').forEach(b=>b.onclick=()=>{view=b.dataset.view;render();});
  document.querySelectorAll('[data-nav]').forEach(b=>b.onclick=()=>{const map={connectors:'connectors',chat:'call',capability:'capabilities',monitor:'monitor',settings:'settings'};if(map[b.dataset.nav]){view=map[b.dataset.nav];render();}});
  document.querySelectorAll('[data-destination]').forEach(b=>b.onclick=()=>{if(b.dataset.destination==='plugin')view='pluginfeature';else if(['monitor','settings'].includes(b.dataset.destination))view=b.dataset.destination;else{destination=b.dataset.destination;view='destination';}render();});
  document.querySelectorAll('[data-node]').forEach(n=>{n.onclick=()=>editNode(n.dataset.node);n.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();editNode(n.dataset.node);}};});
  document.querySelectorAll('[data-filter]').forEach(b=>b.onclick=()=>{boards.find(x=>x.id===activeBoard).mode=b.dataset.filter;render();});
  document.querySelectorAll('[data-manage]').forEach(b=>b.onclick=canvasManager);
  document.querySelectorAll('[data-display]').forEach(b=>b.onclick=displayEditor);
  document.querySelectorAll('[data-create-board]').forEach(b=>b.onclick=createBoard);
  const boardChoice=document.querySelector('[data-board-choice]');if(boardChoice)boardChoice.onchange=()=>{activeBoard=boardChoice.value;render();};
  const add=document.querySelector('[data-add-connection]');if(add)add.onclick=()=>editNode('capability');
  if(view==='capabilities')document.getElementById('primary-dialog').style.display='none';
  document.querySelectorAll('[data-close]').forEach(b=>b.onclick=()=>b.closest('.dialog-overlay').style.display='none');
  document.querySelectorAll('[data-open-cap],[data-open-profile]').forEach(b=>b.onclick=()=>{document.getElementById('primary-dialog').style.display='flex';if(b.dataset.openCap)selectCapabilityCategory(b.dataset.openCap);});
  document.querySelectorAll('[data-import]').forEach(b=>b.onclick=()=>{const layer=openEditor('导入与本地发现','<div class="import-options"><button>本地 Skill 目录</button><button>ZIP / Git 仓库</button><button>工作流 / MCP 配置</button><button>发现本地服务</button></div>','<button class="primary" data-editor-save>选择来源</button>');layer.querySelector('[data-editor-save]').onclick=()=>layer.remove();});
  document.querySelectorAll('[data-cap-tab]').forEach(b=>b.onclick=()=>selectCapabilityCategory(b.textContent));
  const collapse=document.querySelector('[data-collapse]');if(collapse)collapse.onclick=()=>document.querySelector('.call-panel').style.display='none';
  const expand=document.querySelector('[data-expand]');if(expand)expand.onclick=()=>document.querySelector('.call-panel').style.display='block';
  const mute=document.querySelector('[data-mute]');if(mute)mute.onclick=()=>{const muted=mute.dataset.muted!=='true';mute.dataset.muted=String(muted);mute.style.background=muted?'var(--rose-soft)':'var(--soft)';document.querySelector('.call-panel > p').textContent=muted?'麦克风已静音':'正在听你说 · 梳理产品交互';};
  const more=document.querySelector('[data-more]');if(more)more.onclick=()=>{const el=document.createElement('div');el.className='dialog-overlay';el.style.zIndex='10';el.innerHTML=`<section class="dialog reading-dialog"><header class="dialog-header"><h2>文档整理 · 完整说明</h2><button class="iconbtn">${icon('close')}</button></header><div class="dialog-body">整理讨论、笔记和资料，输出结构清楚、来源明确的文档。<br><br>使用方式<br>向小涟说明目标、读者和所需格式，再提供资料。小涟读取文件，整理关键内容，并将结果保存到工作区。<br><br>可以编辑说明和所需工具，并将此 Skill 连接到不同角色的智能体。</div></section>`;el.querySelector('button').onclick=()=>el.remove();document.querySelector('.main').appendChild(el);};
  updateCanvas();
  bindPoolPreview();
}
document.getElementById('theme-toggle').onclick=()=>{dark=!dark;render();};
window.preview={setView:(v,theme='light')=>{view=v;dark=theme==='dark';render();},setModelRouteState:(state)=>{poolRouteState=state;view='settings';render();}};
render();
