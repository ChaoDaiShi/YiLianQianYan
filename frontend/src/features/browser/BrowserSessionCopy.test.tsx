import {describe,it,expect} from "vitest";
import {renderToStaticMarkup} from "react-dom/server";
import BrowserStartPage from "./BrowserStartPage";
import BrowserTools from "./BrowserTools";
import type {BrowserTab} from "../../api/browser";

describe("native profile persistence copy",()=>{
 it("explains that normal storage retains data while website login expiry remains website controlled",()=>{
  const html=renderToStaticMarkup(<BrowserStartPage busy={false} sites={[]} sitesLoading={false} sitesError="" address="" addressRef={{current:null}} onAddress={()=>{}} temporary={false} onTemporary={()=>{}} onNavigate={()=>{}} onOpenSite={()=>{}} onManageSites={()=>{}}/>);
  expect(html).toContain("登录有效期由网站决定");
  expect(html).not.toContain("关闭应用后仍保留");
 });
 it("does not guarantee that session-only website cookies survive an application exit",()=>{
  const tab={ephemeral:false} as BrowserTab;
  const html=renderToStaticMarkup(<BrowserTools tab={tab} zoom={1} busy={false} error="" onZoom={()=>{}} onFind={()=>{}} onBookmark={()=>{}} onClear={async()=>{}} onClose={()=>{}}/>);
  expect(html).toContain("会话 Cookie");
  expect(html).toContain("仍可能需要重新登录");
  expect(html).not.toContain("退出应用后仍保存登录状态");
 });
});
