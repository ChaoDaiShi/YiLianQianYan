//! Host-authored operations run in a separate WebView2 JavaScript world.
use serde_json::{json, Value};

const FILL_CREDENTIAL: &str = r#"
(url, origin, username, password) => {
  if (window !== window.top || location.href !== url || location.origin !== origin) return false;
  const usable = input => input.isConnected && !input.disabled && !input.readOnly
    && input.getClientRects().length > 0 && getComputedStyle(input).visibility === 'visible';
  const passwords = [...document.querySelectorAll('input[type=password]')]
    .filter(input => usable(input) && input.autocomplete !== 'new-password');
  if (passwords.length !== 1) return false;
  const field = passwords[0];
  const form = field.form;
  if (form && new URL(form.action, location.href).origin !== origin) return false;
  const text = [...(form ? form.elements : document.querySelectorAll('input'))]
    .filter(input => input instanceof HTMLInputElement && ['text','email','tel'].includes(input.type) && usable(input));
  const hinted = text.filter(input => /(?:^|\s)(?:username|email)(?:$|\s)/i.test(input.autocomplete));
  const candidates = hinted.length ? hinted : text;
  if (candidates.length !== 1) return false;
  const account = candidates[0];
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
  setter.call(account, username);
  setter.call(field, password);
  for (const input of [account, field]) {
    EventTarget.prototype.dispatchEvent.call(input, new Event('input', { bubbles: true }));
    EventTarget.prototype.dispatchEvent.call(input, new Event('change', { bubbles: true }));
  }
  if (account.value !== username || field.value !== password) throw new Error('Credential fill could not be confirmed');
  return true;
}
"#;

pub fn fill_credential(url: &str, origin: &str, username: &str, password: &str) -> String {
    format!(
        "({FILL_CREDENTIAL})({},{},{},{})",
        json!(url),
        json!(origin),
        json!(username),
        json!(password)
    )
}
const BRIDGE: &str = r#"
if(!globalThis.__yilianNativeBrowser){globalThis.__yilianNativeBrowser=(()=>{
  let revision=0; const observations=new Map();
  const observer=new MutationObserver(records=>{if(records.length)revision++;});
  observer.observe(document,{subtree:true,childList:true,attributes:true,characterData:true});
  const drain=()=>{if(observer.takeRecords().length)revision++;};
  addEventListener("input",()=>revision++,true);addEventListener("change",()=>revision++,true);
  const visible=n=>{const r=n.getBoundingClientRect();return n.isConnected&&n.getClientRects().length&&!n.disabled&&r.bottom>0&&r.top<innerHeight&&r.right>0&&r.left<innerWidth;};
  function observe(token){drain();const nodes=[...document.querySelectorAll('a[href],button,input,textarea,select,[role="button"],[role="link"],[contenteditable="true"]')].filter(visible).slice(0,150);observations.clear();observations.set(token,{revision,nodes:nodes.map(node=>({node,value:String(node.value??""),type:String(node.type??"")}))});return {ok:true,observation_id:token,dom_revision:revision,url:location.href,title:document.title,text:(document.body?.innerText||'').slice(0,24000),document_ready:document.readyState,elements:nodes.map((n,i)=>({element_id:'e'+i,tag:n.tagName.toLowerCase(),role:n.getAttribute('role')||'',name:(n.getAttribute('aria-label')||n.getAttribute('placeholder')||n.innerText||n.getAttribute('name')||'').slice(0,240),input_type:n.type||'',value:n.type==='password'?'[redacted]':String(n.value||'').slice(0,240)}))};}
  function action(kind,token,id,text){drain();const saved=observations.get(token);if(!saved||saved.revision!==revision)return {ok:false,no_effect:true,error:'Observation expired after a page change'};const prior=saved.nodes[Number(id.slice(1))];const n=prior?.node;if(!n||String(n.value??"")!==prior.value||String(n.type??"")!==prior.type||!visible(n))return {ok:false,no_effect:true,error:'Observation target is stale'};if(n.type==='file'||n.type==='password')return {ok:false,no_effect:true,error:'File and password inputs are not available to browser tools'};observations.clear();if(kind==='click'){Element.prototype.scrollIntoView.call(n,{block:'center'});HTMLElement.prototype.click.call(n);return {ok:true,clicked:true,business_result_verified:false};}if(kind==='fill'){if(n.tagName!=='INPUT'&&n.tagName!=='TEXTAREA')return {ok:false,no_effect:true,error:'Target is not a text input'};const proto=n.tagName==='TEXTAREA'?HTMLTextAreaElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,'value').set.call(n,text);EventTarget.prototype.dispatchEvent.call(n,new Event('input',{bubbles:true}));EventTarget.prototype.dispatchEvent.call(n,new Event('change',{bubbles:true}));return {ok:n.value===text,filled:n.value===text};}return {ok:false,no_effect:true,error:'Unsupported browser action'};}
  function scroll(token,x,y){drain();const saved=observations.get(token);if(!saved||saved.revision!==revision)return {ok:false,no_effect:true,error:'Observation expired before scrolling'};observations.clear();const oldX=scrollX,oldY=scrollY;scrollBy(x,y);return {ok:true,x:scrollX,y:scrollY,moved:oldX!==scrollX||oldY!==scrollY};}
  return {observe,action,scroll};
})();}
"#;
pub fn observe(token: &str) -> String {
    format!("(()=>{{try{{{BRIDGE}return globalThis.__yilianNativeBrowser.observe({});}}catch(e){{return {{ok:false,error:String(e).slice(0,300)}};}}}})()",json!(token))
}
pub fn action(kind: &str, token: &str, element: &str, payload: &Value) -> Result<String, String> {
    if !element.starts_with('e')
        || element.len() > 5
        || element.len() < 2
        || !element[1..].chars().all(|c| c.is_ascii_digit())
    {
        return Err("Invalid observation element".into());
    }
    let text = payload
        .get("value")
        .or_else(|| payload.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if text.chars().count() > 16384 {
        return Err("Input exceeds 16384 characters".into());
    }
    Ok(format!("(()=>{{try{{{BRIDGE}return globalThis.__yilianNativeBrowser.action({},{},{},{});}}catch(e){{return {{ok:false,error:String(e).slice(0,300)}};}}}})()",json!(kind),json!(token),json!(element),json!(text)))
}
pub fn scroll(token: &str, x: i64, y: i64) -> String {
    format!("(()=>{{try{{{BRIDGE}return globalThis.__yilianNativeBrowser.scroll({},{},{});}}catch(e){{return {{ok:false,error:String(e).slice(0,300)}};}}}})()",json!(token),x.clamp(-4000,4000),y.clamp(-4000,4000))
}
