import { useEffect,useState,type ReactNode } from "react";
import { BrainCircuit,Mic,Volume2,Search,ScanEye,ImagePlus,Database } from "lucide-react";
import { Button,Input,Modal,Select } from "../../../components/ui";
import { managementRequest } from "../../../api/productManagement";
import { listProviderAccounts, type ProviderAccount } from "../../llm/modelPools";
import BasicServicePanel from "./BasicServicePanel";
import SettingsDialogLayout from "./SettingsDialogLayout";
type Kind="chat"|"stt"|"tts"|"search"|"vision"|"image"|"embedding";
const cards=[{id:"chat",title:"对话模型",description:"选择厂商、模型和密钥，直接开始对话",icon:BrainCircuit},{id:"stt",title:"语音识别",description:"将麦克风音频转换为文字",icon:Mic},{id:"tts",title:"语音合成",description:"配置朗读模型、音色与试听",icon:Volume2},{id:"search",title:"联网搜索",description:"查询公开信息并返回来源",icon:Search},{id:"vision",title:"图像识别",description:"上传图像，验证真实视觉理解",icon:ScanEye},{id:"image",title:"图像生成",description:"从提示词生成可查看的图像",icon:ImagePlus},{id:"embedding",title:"向量模型",description:"独立配置知识检索的向量空间",icon:Database}] as const;
interface Props {modelPanel:ReactNode;renderVoice:(side:"stt"|"tts")=>ReactNode;embeddingPanel:ReactNode;onSave?:()=>Promise<void>;saving?:boolean;dirty?:boolean}
const searchProviders = [
  { value: "anysearch", label: "AnySearch", baseUrl: "https://api.anysearch.com" },
  { value: "tavily", label: "Tavily", baseUrl: "https://api.tavily.com" },
  { value: "brave", label: "Brave Search", baseUrl: "https://api.search.brave.com" },
  { value: "bocha", label: "博查", baseUrl: "https://api.bochaai.com" },
  { value: "deepseek", label: "DeepSeek 搜索 API", baseUrl: "https://api.deepseek.com/v1" },
];

interface SearchConfig {
  provider: string;
  base_url: string;
  configured: boolean;
  provider_account_id: string | null;
}

export function SearchPanel({ onClose }: { onClose?: () => void } = {}) {
  const [config, setConfig] = useState<SearchConfig | null>(null);
  const [savedConfig, setSavedConfig] = useState<SearchConfig | null>(null);
  const [accounts, setAccounts] = useState<ProviderAccount[]>([]);
  const [key, setKey] = useState("");
  const [clear, setClear] = useState(false);
  const [query, setQuery] = useState("忆涟千言 桌面智能体");
  const [results, setResults] = useState<Array<{ title: string; url: string; snippet?: string }> | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState<"save" | "test" | null>(null);
  const deepseekAccounts = accounts.filter((account) => account.provider_id === "deepseek");

  useEffect(() => {
    let active = true;
    void Promise.all([managementRequest<SearchConfig>("GET", "/api/search/config"), listProviderAccounts()])
      .then(([saved, availableAccounts]) => {
        if (active) {
          setConfig(saved);
          setSavedConfig(saved);
          setAccounts(availableAccounts);
        }
      })
      .catch((cause) => { if (active) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { active = false; };
  }, []);

  async function run(operation: "save" | "test", action: () => Promise<void>) {
    setBusy(operation);
    setError("");
    setNotice("");
    try {
      await action();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(null);
    }
  }

  async function save() {
    if (!config) return;
    await run("save", async () => {
      const saved = await managementRequest<SearchConfig>("PUT", "/api/search/config", {
        provider: config.provider,
        base_url: config.base_url,
        provider_account_id: config.provider_account_id,
        credential_mode: config.provider_account_id ? "account" : "direct",
        api_key: key,
        clear_api_key: clear,
      });
      setConfig(saved);
      setSavedConfig(saved);
      setKey("");
      setClear(false);
      setNotice("搜索配置已保存");
    });
  }

  const unsaved = Boolean(key || clear || (config && savedConfig && (
    config.provider !== savedConfig.provider || config.base_url !== savedConfig.base_url || config.provider_account_id !== savedConfig.provider_account_id
  )));
  return (
    <SettingsDialogLayout title="联网搜索" onClose={onClose} closeDisabled={busy === "save"} unsaved={unsaved} error={error} notice={notice}
      action={<Button disabled={!config || busy !== null} onClick={() => void save()}>{busy === "save" ? "正在保存…" : "保存搜索配置"}</Button>}>
      {!config ? !error && <p className="settings-help" role="status">正在读取搜索配置…</p> : (
        <fieldset className="min-w-0 space-y-3" disabled={busy !== null}>
          <Select label="搜索厂商" options={searchProviders} value={config.provider} onChange={(event) => {
            const provider = searchProviders.find((item) => item.value === event.target.value);
            if (!provider) return;
            setConfig({ provider: provider.value, base_url: provider.baseUrl, configured: false, provider_account_id: null });
            setKey("");
            setClear(false);
            setNotice("");
          }} />
          {config.provider === "deepseek" && (deepseekAccounts.length > 0 || config.provider_account_id) && (
            <Select label="账户" value={config.provider_account_id ?? ""} options={[
              { value: "", label: "输入搜索密钥" },
              ...deepseekAccounts.map((account) => ({ value: account.id, label: account.label })),
            ]} onChange={(event) => {
              const account = deepseekAccounts.find((item) => item.id === event.target.value);
              setConfig({ ...config, provider_account_id: account?.id ?? null, base_url: account?.base_url ?? "https://api.deepseek.com/v1", configured: account?.credential_configured ?? false });
              setNotice("");
            }} />
          )}
          {!config.provider_account_id && <Input label="API Key" type="password" autoComplete="new-password" value={key}
            onChange={(event) => { setKey(event.target.value); setNotice(""); }}
            hint={config.configured ? "密钥已配置，留空沿用。" : undefined} />}

          <details className="u2-disclosure">
            <summary>高级选项</summary>
            <div className="space-y-3 pt-2">
              <Input label="API 地址" value={config.base_url} onChange={(event) => {
                setConfig({ ...config, base_url: event.target.value });
                setNotice("");
              }} />
              {!config.provider_account_id && <label className="flex gap-2 text-sm">
                <input type="checkbox" checked={clear} onChange={(event) => { setClear(event.target.checked); setNotice(""); }} />清除搜索凭据
              </label>}
              {config.provider === "deepseek" && <p className="settings-help">使用 DeepSeek 独立搜索接口。</p>}
            </div>
          </details>

          <details className="u2-disclosure">
            <summary>测试搜索</summary>
            <div className="space-y-3 pt-2">
              <Input label="测试查询" value={query} onChange={(event) => setQuery(event.target.value)} hint="使用已保存的搜索配置。" />
              <Button variant="secondary" size="sm" disabled={busy !== null || !query.trim()} onClick={() => void run("test", async () => {
                setResults(null);
                const response = await managementRequest<{ results: NonNullable<typeof results> }>("POST", "/api/search", { query, max_results: 3 });
                setResults(response.results);
              })}>{busy === "test" ? "正在搜索…" : "发送真实搜索请求"}</Button>
              {results?.length === 0 && <p className="settings-help" role="status">没有搜索结果。</p>}
              {results?.map((result) => <article key={result.url} className="glass-card break-words rounded-lg border border-[var(--border-soft)] p-3">
                <a href={result.url} target="_blank" rel="noreferrer" className="font-medium underline">{result.title}</a>
                {result.snippet && <p className="text-sm">{result.snippet}</p>}
              </article>)}
            </div>
          </details>
        </fieldset>
      )}
    </SettingsDialogLayout>
  );
}
export default function BasicConfiguration({modelPanel,renderVoice,embeddingPanel,onSave,saving=false,dirty=false}:Props){const[active,setActive]=useState<Kind|null>(null);const title=cards.find(c=>c.id===active)?.title??"基础配置";return <><div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3">{cards.map(card=><button key={card.id} type="button" onClick={()=>setActive(card.id)} className="glass-card flex items-start gap-3 rounded-2xl border border-[var(--border-soft)] bg-[var(--surface-solid)] p-5 text-left transition hover:border-[var(--accent-primary)]"><card.icon className="mt-1 shrink-0 text-[var(--accent-primary)]" size={24}/><span><strong className="block text-base">{card.title}</strong><span className="mt-1 block text-sm text-[var(--text-muted)]">{card.description}</span></span></button>)}</div><Modal open={active!==null} onClose={()=>setActive(null)} title={title} size="xl" footer={<div className="flex justify-end gap-2"><Button onClick={()=>setActive(null)}>关闭</Button>{onSave&&(active==="stt"||active==="tts"||active==="embedding")&&<Button disabled={saving||!dirty} onClick={()=>void onSave()}>{saving?"正在保存…":"保存配置"}</Button>}</div>}>{active==="chat"?modelPanel:active==="stt"||active==="tts"?renderVoice(active):active==="embedding"?embeddingPanel:active==="search"?<SearchPanel/>:active==="vision"||active==="image"?<BasicServicePanel key={active} kind={active}/>:null}</Modal></>}
