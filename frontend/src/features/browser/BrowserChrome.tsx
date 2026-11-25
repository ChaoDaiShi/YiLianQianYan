import { ArrowLeft, ArrowRight, Bookmark, ExternalLink, History, KeyRound, MoreHorizontal, Plus, RotateCw, Square, X } from "lucide-react";
import type { RefObject } from "react";
import { Button } from "../../components/ui";
import type { BrowserTab, ProjectSite } from "../../api/browser";
import type { Workspace } from "../../api/workspaces";
import { siteIdentityLabel } from "./browserPolicy";

interface BrowserChromeProps {
  detached: boolean;
  tabs: BrowserTab[];
  selected: string;
  current?: BrowserTab;
  address: string;
  project: string;
  workspaces: Workspace[];
  sites: ProjectSite[];
  busy: boolean;
  available: boolean;
  starting: boolean;
  unknownCount: number;
  projectName?: string;
  onSelect: (tab: BrowserTab) => void;
  onStart: () => void;
  onAddress: (address: string) => void;
  onProject: (id: string) => void;
  onNavigate: () => void;
  onAction: (action: string) => void;
  onManageSites: () => void;
  onReview: () => void;
  onOpenSite: (site: ProjectSite) => void;
  onExternal: () => void;
  onMove: () => void;
  onCloseTab: (tab:BrowserTab) => void;
  onHistory: () => void;
  onTools: () => void;
  onCredentialVault: () => void;
  addressRef: RefObject<HTMLInputElement>;
}

export default function BrowserChrome(props: BrowserChromeProps) {
  const { current, busy, available, detached, starting } = props;
  const loading = current?.loading === true;
  const RefreshIcon = loading ? Square : RotateCw;
  return (
    <>
      <header className="browser-header">
        <strong className="browser-title">浏览器</strong>
        <div className="browser-tab-strip" role="tablist" aria-label="网页会话">
          {props.tabs.map(tab => (
            <div key={tab.tab_id} className="browser-tab-group" data-selected={props.selected===tab.tab_id}>
            <button role="tab" aria-selected={props.selected === tab.tab_id}
              className="browser-tab" onClick={() => props.onSelect(tab)} title={tab.url}>
              {tab.title || "新页面"}{tab.mode === "detached" ? " ↗" : ""}
            </button>
            <button type="button" className="browser-tab-close" aria-label={`关闭网页 ${tab.title||tab.url}`} title="关闭网页" disabled={busy} onClick={()=>props.onCloseTab(tab)}><X size={12}/></button>
            </div>
          ))}
        </div>
        {!detached && <Button size="sm" variant="ghost" iconOnly aria-label="新建标签页" title="新建标签页（Ctrl+T）" onClick={props.onStart}><Plus size={16} /></Button>}
        {available && <Button size="sm" variant="ghost" iconOnly aria-label="密码箱" title="密码箱" onClick={props.onCredentialVault}><KeyRound size={16} /></Button>}
        {!starting && <Button size="sm" variant="secondary" onClick={props.onManageSites}><Bookmark size={14} /><span className="browser-sites-label">项目站点</span></Button>}
        {props.unknownCount > 0 && <Button variant="secondary" size="sm" onClick={props.onReview}>待审阅操作（{props.unknownCount}）</Button>}
      </header>
      <form className="browser-toolbar" onSubmit={event => { event.preventDefault(); props.onNavigate(); }}>
        {current && <div className="browser-nav-buttons">
          {([
            ["back", "后退", ArrowLeft], ["forward", "前进", ArrowRight],
          ] as const).map(([name, label, Icon]) => (
            <Button key={name} variant="ghost" size="sm" iconOnly aria-label={label} title={label}
              disabled={busy || (name === "back" && current.can_go_back === false) || (name === "forward" && current.can_go_forward === false)}
              onClick={() => props.onAction(name)}><Icon size={16} /></Button>
          ))}
          <Button variant="ghost" size="sm" iconOnly aria-label={loading ? "停止加载" : "刷新"}
            title={loading ? "停止加载" : "刷新"} disabled={busy && !loading}
            onClick={() => props.onAction(loading ? "stop" : "reload")}><RefreshIcon size={16} /></Button>
        </div>}
        {!starting && (!detached || current) && <div className="browser-omnibox">
          <input ref={props.addressRef} aria-label="网址" className="ui-input browser-address" value={props.address}
            onChange={event => props.onAddress(event.target.value)} placeholder="输入网址或关键词（Bing 搜索）" autoComplete="off" />
          <Button size="sm" type="submit" disabled={busy || !available || !props.address.trim()}>前往</Button>
        </div>}
        <div className="browser-session-controls">
          <Button variant="ghost" size="sm" iconOnly aria-label="浏览历史" title="浏览历史" onClick={props.onHistory}><History size={15}/></Button>
          {current && <Button variant="ghost" size="sm" iconOnly aria-label="浏览器工具" title="浏览器工具" onClick={props.onTools}><MoreHorizontal size={17}/></Button>}
          {!current && <select aria-label="浏览项目" className="ui-input browser-project" value={props.project}
            onChange={event => props.onProject(event.target.value)}>
            <option value="">不关联项目</option>
            {props.workspaces.map(workspace => <option key={workspace.id} value={workspace.id}>{workspace.name}</option>)}
          </select>}
          {!detached && <Button variant="ghost" size="sm" iconOnly aria-label="系统浏览器" title="系统浏览器" disabled={!props.address.trim()} onClick={props.onExternal}><ExternalLink size={16} /></Button>}
          {current && (!detached || current.mode !== "embedded") && (
            <Button variant="secondary" size="sm" disabled={busy} onClick={props.onMove}>
              <ExternalLink size={14} />{detached || current.mode === "detached" ? "回到主窗口" : "独立窗口"}
            </Button>
          )}
        </div>
      </form>
      {!starting && <div className="browser-utility-row">
        {props.sites.length > 0 && <nav className="browser-bookmarks" aria-label="项目站点快捷方式">
          {props.sites.slice(0, 5).map(site => <button key={site.id} disabled={busy || !available} onClick={() => props.onOpenSite(site)} title={site.url}><Bookmark size={12} /><span>{site.title}</span></button>)}
        </nav>}
        <div className="browser-identity" role="status" title={[siteIdentityLabel(props.projectName), current && `会话 ${current.session_id} / 页面 ${current.navigation_id}`].filter(Boolean).join("\n")}>
          <span className="browser-identity-project">{props.projectName || "未关联项目"}</span>
          <span> · 服务归属未核实{current ? current.ephemeral?" · 临时浏览":" · 保留登录":""}{current?.loading ? " · 正在加载" : ""}</span>
        </div>
      </div>}
    </>
  );
}
