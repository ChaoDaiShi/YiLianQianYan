import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import AccountCredentialFields from "./AccountCredentialFields";
import type { LlmModel } from "../../types";

describe("account credential fields", () => {
  it("exposes a missing account's key input and only explicitly eligible shared reference", () => {
    const models=[{id:"stored",provider:"deepseek",label:"已保存",model:"deepseek-chat",base_url:"https://api.deepseek.com",api_key_configured:true,api_key_source:"secret_store"},{id:"other",provider:"openai",label:"其他厂商",model:"gpt",base_url:"https://api.deepseek.com",api_key_configured:true,api_key_source:"secret_store"}] as LlmModel[];
    const html=renderToStaticMarkup(<AccountCredentialFields account={{id:"account",provider_id:"deepseek",label:"账户",base_url:"https://api.deepseek.com",credential_id:null,profile_ids:[],credential_status:"missing",credential_configured:false}} provider="deepseek" baseUrl="https://api.deepseek.com" models={models} apiKey="" modelId="stored" onKey={vi.fn()} onModel={vi.fn()}/>);
    expect(html).toContain("凭据未配置");expect(html).toContain("API Key");expect(html).toContain("deepseek-chat");expect(html).not.toContain("其他厂商");expect(html).toContain("共享同一凭据引用");expect(html).toContain("影响使用该引用的模型和账户");
  });
});
