// Design-only state for reviewing provider accounts, ordered pools and notifications.
let poolRouteState = "primary";
const poolAccounts = [
  {id:"deepseek",name:"DeepSeek",alias:"日常账户",models:"DeepSeek 对话模型",symbol:"D"},
  {id:"qwen",name:"阿里云百炼",alias:"写作账户",models:"通义千问",symbol:"Q"},
  {id:"openai",name:"OpenAI",alias:"通用账户",models:"OpenAI 对话模型",symbol:"O"},
];
const poolTiers = [
  {id:"tier-a",account:"deepseek",model:"DeepSeek 对话档案",description:"日常对话与工具调用"},
  {id:"tier-b",account:"qwen",model:"通义千问对话档案",description:"首选不可用时接续"},
  {id:"tier-c",account:"openai",model:"OpenAI 对话档案",description:"后续候选"},
];
function modelPoolSettings(){
  const fallback=poolRouteState==="fallback",exhausted=poolRouteState==="exhausted";
  const rows=poolTiers.map((tier,index)=>{
    const account=poolAccounts.find(a=>a.id===tier.account);
    const active=!exhausted&&index===(fallback?1:0),failed=exhausted||(fallback&&index===0);
    return `<div class="pool-tier ${active?'current':''} ${failed?'failed':''}" data-tier="${tier.id}">
      <div class="tier-order"><span>L${index+1}</span><small>${index===0?'首选':`第 ${index+1} 级`}</small></div>
      <span class="provider-logo">${account.symbol}</span><div class="tier-copy"><h3>${tier.model}</h3><p>${account.name} · ${account.alias} · ${tier.description}</p></div>
      <span class="pill ${active?'green':failed?'warning':'purple'}">${active?'正在使用':failed?'连接超时':'待命'}</span>
      <div class="tier-actions"><button data-tier-up="${tier.id}" title="上移一级" ${index===0?'disabled':''}>↑</button><button data-tier-down="${tier.id}" title="下移一级" ${index===poolTiers.length-1?'disabled':''}>↓</button><button data-tier-edit="${tier.id}" title="编辑候选">${icon('settings')}</button></div>
    </div>`;
  }).join('');
  const notice=fallback?`<div class="pool-notice switched" role="status">${icon('arrow')}<div><strong>已从一级降级到二级 · 通义千问</strong><p>DeepSeek 连接超时，本轮改用通义千问继续回答。</p></div><button class="textbtn" data-route-record>查看切换记录</button></div>`:exhausted?`<div class="pool-notice switched" role="status">${icon('monitor')}<div><strong>本轮所有候选均不可用</strong><p>已按顺序尝试 ${poolTiers.length} 级模型。请检查连接后重试。</p></div><button class="textbtn" data-route-record>查看原因</button></div>`:`<div class="pool-notice" role="status">${icon('check')}<div><strong>自动降级已开启 · 按层级顺序调用</strong><p>当前使用一级模型；不可用时尝试下一候选，并显示切换提示。</p></div><button class="textbtn" data-route-record>调用顺序 →</button></div>`;
  return shell(`<section class="page">${heading('PREFERENCES','基础配置','添加多家厂商，按优先级组织模型。')}<div class="settings-layout"><aside class="settings-nav">${['基础配置','安全控制','智能体偏好','能力目录','外观'].map((x,i)=>`<button class="${i===0?'selected':''}">${x}</button>`).join('')}</aside>
    <div class="settings-content pool-workbench"><div class="pool-section-title"><h2>已添加厂商 <span class="pill">${poolAccounts.length} 家</span></h2><button class="primary" data-add-provider>${icon('plus')} 添加厂商</button></div>
    <div class="configured-providers">${poolAccounts.map(a=>`<button class="provider-account panel" data-account="${a.id}"><span class="provider-logo">${a.symbol}</span><div><h3>${a.name}</h3><p>${a.alias}</p></div><span class="pill green">已配置</span></button>`).join('')}</div>
    <section class="panel pool-panel"><header class="pool-header"><div><span class="eyebrow">MODEL POOL</span><h2>日常对话池 <span class="pill">已绑定小涟</span></h2></div><div class="actions"><button data-edit-pool>池设置</button><button data-add-tier>${icon('plus')} 添加层级</button></div></header>
    <div class="pool-tiers">${rows}</div><footer class="pool-footer"><span>同层按顺序尝试 · 当前层不可用后进入下一级</span><span>下一次请求优先尝试一级</span></footer></section>
    ${notice}<div class="other-services"><span>其他基础配置</span>${['语音识别','语音合成','搜索','识图','生图','Embedding'].map(label=>`<button data-service="${label}">${label} →</button>`).join('')}</div>
    </div></div></section>`,'settings','基础配置');
}
function pluginFeaturePage(){
  return shell(`<section class="page">${heading('PLUGIN FEATURE','文档工作台','来自文档助手插件 · 工作流功能',`<button data-plugin-settings>${icon('settings')} 管理插件</button>`)}
    <div class="plugin-workspace"><div class="panel plugin-feature-header"><span class="nav-card-icon">${icon('knowledge')}</span><div><h2>把资料整理成一份清楚的文档</h2><p>选择来源、指定输出方式，交给文档整理工作流。</p></div><span class="pill rose">文档助手插件</span></div>
    <div class="plugin-feature-grid"><section class="panel feature-sources"><h3>资料来源</h3><div class="upload-design">${icon('import')}<h3>添加需要整理的资料</h3><p>支持文档、笔记和本地文件</p><button data-plugin-add-source>选择资料</button></div></section><section class="panel feature-options"><h3>整理方式</h3><div class="field"><label>输出内容</label><select class="input"><option>结构化文档</option><option>摘要与要点</option></select></div><div class="field"><label>使用智能体</label><select class="input"><option>小涟 · 日常助手</option></select></div><div class="field"><label>输出目录</label><div class="input">当前工作空间 / 文档</div></div><span class="pill">沿用智能体权限与模型池</span></section></div>
    </div></section>`,'plugin','文档工作台');
}
function providerAccountEditor(account){
  const editing=Boolean(account);
  const layer=openEditor(editing?'编辑厂商账户':'添加厂商',`<div class="field"><label>官方厂商</label><select class="input" id="provider-name">${['DeepSeek','阿里云百炼','OpenAI','MiniMax','Google Gemini','Anthropic Claude','Moonshot Kimi','火山方舟'].map(name=>`<option ${account?.name===name?'selected':''}>${name}</option>`).join('')}</select></div><div class="field"><label>账户名称</label><input class="input" id="provider-alias" value="${account?.alias||''}" placeholder="例如：个人账户、工作账户"></div><div class="field"><label>访问凭据</label><input class="input" type="password" placeholder="${editing?'已有凭据已保存；可在此替换':'粘贴此厂商的 API Key'}"></div><div class="editor-notice">各厂商账户独立保存。添加后可创建多个模型档案，再加入模型池。</div>`,'<button class="primary" data-editor-save>保存厂商</button>');
  layer.querySelector(".dialog-header p").textContent="可以同时添加多家厂商，不会覆盖已有账户。";
  layer.querySelector(".dialog-footer > span").textContent="基础配置 / 厂商账户";
  layer.querySelector("[data-editor-save]").onclick=()=>{
    const name=layer.querySelector("#provider-name").value,alias=layer.querySelector("#provider-alias").value||"新账户";
    if(editing){account.name=name;account.alias=escapeText(alias);}else poolAccounts.push({id:`account-${Date.now()}`,name,alias:escapeText(alias),models:"未添加模型",symbol:name.slice(0,1)});
    layer.remove();render();
  };
}
function tierEditor(id){
  const tier=poolTiers.find(t=>t.id===id);
  const layer=openEditor(tier?'编辑模型候选':'添加模型层级',`<div class="field"><label>已配置厂商</label><select class="input" id="tier-account">${poolAccounts.map(a=>`<option value="${a.id}" ${tier?.account===a.id?'selected':''}>${a.name} · ${a.alias}</option>`).join('')}</select></div><div class="field"><label>模型档案名称</label><input class="input" id="tier-model" value="${tier?.model||''}" placeholder="选择或填写已经配置的模型档案"></div><div class="editor-notice">保存到${tier?'当前':'新建的最后一个'}层级。可以使用上下箭头调整优先级。</div>`,'<button class="primary" data-editor-save>保存到模型池</button>');
  layer.querySelector(".dialog-footer > span").textContent="日常对话池";
  layer.querySelector("[data-editor-save]").onclick=()=>{const account=layer.querySelector("#tier-account").value,model=escapeText(layer.querySelector("#tier-model").value||"新模型档案");if(tier){tier.account=account;tier.model=model;}else poolTiers.push({id:`tier-${Date.now()}`,account,model,description:"按此顺序调用"});render();};
}
function bindPoolPreview(){
  document.querySelectorAll("[data-add-provider]").forEach(b=>b.onclick=()=>providerAccountEditor(null));
  document.querySelectorAll("[data-account]").forEach(b=>b.onclick=()=>providerAccountEditor(poolAccounts.find(a=>a.id===b.dataset.account)));
  document.querySelectorAll("[data-add-tier]").forEach(b=>b.onclick=()=>tierEditor(null));
  document.querySelectorAll("[data-tier-edit]").forEach(b=>b.onclick=()=>tierEditor(b.dataset.tierEdit));
  document.querySelectorAll("[data-tier-up],[data-tier-down]").forEach(b=>b.onclick=()=>{const id=b.dataset.tierUp||b.dataset.tierDown,index=poolTiers.findIndex(t=>t.id===id),next=index+(b.dataset.tierUp?-1:1);[poolTiers[index],poolTiers[next]]=[poolTiers[next],poolTiers[index]];poolRouteState="primary";render();});
  document.querySelectorAll("[data-route-record]").forEach(b=>b.onclick=()=>{const layer=openEditor("模型调用顺序",`<div class="route-record">${poolTiers.map((t,i)=>`<div><span class="pill">L${i+1}</span><strong>${t.model}</strong><p>${poolRouteState==="fallback"&&i===0?'连接超时 · 已切换下一层':poolRouteState==="fallback"&&i===1?'调用成功 · 当前回答来源':'按优先级顺序调用'}</p></div>`).join('')}</div>`,'<button class="primary" data-editor-save>知道了</button>');layer.querySelector(".dialog-header p").textContent="当前池的顺序与本轮模型来源。";layer.querySelector("[data-editor-save]").onclick=()=>layer.remove();});
  document.querySelectorAll("[data-edit-pool]").forEach(b=>b.onclick=()=>{const layer=openEditor("模型池设置",'<div class="field"><label>池名称</label><input class="input" value="日常对话池"></div><div class="field"><label>用途</label><select class="input"><option>对话与工具调用</option><option>视觉理解</option><option>图像生成</option></select></div><div class="editor-notice">候选按层级自动降级；每次切换都显示原因及实际模型。全部不可用时结束本轮。</div>');layer.querySelector("[data-editor-save]").onclick=()=>layer.remove();});
  document.querySelectorAll("[data-service]").forEach(b=>b.onclick=()=>{const layer=openEditor(`${b.dataset.service}配置`,'<div class="field"><label>服务档案</label><select class="input"><option>选择已添加厂商的服务档案</option></select></div>');layer.querySelector("[data-editor-save]").onclick=()=>layer.remove();});
  document.querySelectorAll("[data-plugin-settings]").forEach(b=>b.onclick=()=>{view="capabilities";render();document.querySelector('[data-open-cap="插件"]').click();});
  document.querySelectorAll("[data-plugin-add-source]").forEach(b=>b.onclick=()=>{const layer=openEditor("选择资料",'<div class="field"><label>资料来源</label><select class="input"><option>当前工作区文件</option><option>知识库文档</option></select></div>');layer.querySelector("[data-editor-save]").onclick=()=>layer.remove();});
}
