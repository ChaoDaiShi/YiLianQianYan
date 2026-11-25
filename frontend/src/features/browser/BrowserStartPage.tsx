import type { RefObject } from "react";
import { Bookmark, Globe, Search } from "lucide-react";
import { Button } from "../../components/ui";
import type { ProjectSite } from "../../api/browser";

interface BrowserStartPageProps {
  busy: boolean;
  sites: ProjectSite[];
  sitesLoading: boolean;
  sitesError: string;
  address: string;
  addressRef: RefObject<HTMLInputElement>;
  onAddress: (address: string) => void;
  onNavigate: () => void;
  onOpenSite: (site: ProjectSite) => void;
  onManageSites: () => void;
  temporary: boolean;
  onTemporary: (temporary:boolean) => void;
}

export default function BrowserStartPage({
  busy, sites, sitesLoading, sitesError, address, addressRef, onAddress, onNavigate, onOpenSite, onManageSites, temporary, onTemporary,
}: BrowserStartPageProps) {
  return (
    <div className="browser-start">
      <div className="browser-start-content">
        <h1 className="sr-only">搜索与浏览</h1>
        <form className="browser-start-search" onSubmit={event => {
          event.preventDefault();
          onNavigate();
        }}>
          <Search size={19} aria-hidden="true" />
          <input
            aria-label="搜索网页或输入网址"
            ref={addressRef}
            value={address}
            onChange={event => onAddress(event.target.value)}
            placeholder="搜索网页，或输入网址 / localhost:3000"
            autoComplete="off"
          />
          <Button type="submit" size="sm" disabled={busy || !address.trim()} aria-label="搜索或前往">
            <Search size={17} />
          </Button>
        </form>
        <p className="browser-start-hint">关键词使用 Bing 搜索 · 网址直接打开</p>
        <div className="browser-start-sites" aria-label="收藏的项目站点">
          {sites.map(site => (
            <button className="browser-start-site" key={site.id} disabled={busy} onClick={() => onOpenSite(site)} title={`${site.title}\n${site.url}`}>
              <span className="browser-start-site-icon"><Globe size={21} /></span>
              <span>{site.title}</span>
            </button>
          ))}
          <button className="browser-start-site" onClick={onManageSites}>
            <span className="browser-start-site-icon"><Bookmark size={21} /></span>
            <span>管理项目站点</span>
          </button>
        </div>
        {sitesLoading ? <p role="status" className="browser-start-hint">正在读取项目站点…</p>
          : sitesError ? <p role="alert" className="browser-start-hint">项目站点读取失败：{sitesError}</p>
          : !sites.length ? <p className="browser-start-hint">收藏的项目站点会显示在这里</p> : null}
        <fieldset className="browser-profile-choice" disabled={busy}>
          <legend className="sr-only">浏览方式</legend>
          <label><input type="radio" name="browser-profile" checked={!temporary} onChange={()=>onTemporary(false)}/>保留登录</label>
          <label><input type="radio" name="browser-profile" aria-label="临时浏览" checked={temporary} onChange={()=>onTemporary(true)}/>临时浏览</label>
        </fieldset>
        <p className="browser-start-privacy">{temporary?"临时浏览使用独立会话，关闭后丢弃登录状态。":"普通标签页保留浏览数据，同一项目共享登录资料，不同项目独立存储。登录有效期由网站决定；退出账号、凭据过期或仅会话登录仍可能需要重新登录。"}</p>
      </div>
    </div>
  );
}
