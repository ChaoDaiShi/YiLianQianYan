import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import NativeBrowserNotice from "./NativeBrowserNotice";
describe("native browser nonfatal notices",()=>{
 it("reports a blocked popup without claiming page failure or automatically reloading",()=>{const dismiss=vi.fn();const html=renderToStaticMarkup(<NativeBrowserNotice message="Popup blocked" onDismiss={dismiss}/>);expect(html).toContain("Popup blocked");expect(html).toContain("当前页面保留");expect(html).not.toContain("网页未能加载");expect(html).not.toContain("刷新");expect(dismiss).not.toHaveBeenCalled();});
 it("does not display a warning when there is no native status",()=>{expect(renderToStaticMarkup(<NativeBrowserNotice message="" onDismiss={()=>{}}/>)).toBe("");});
});
