import { describe, expect, it, vi } from "vitest";
import type { LlmModel } from "../../types";
import { credentialChoice, credentialReady, eligibleCredentialModels } from "./accountCredentials";
import { finishQuickModelSetup } from "./quickModelCompletion";

const model = (id: string, values: Partial<LlmModel> = {}): LlmModel => ({ id, provider:"deepseek", label:id, model:id, base_url:"https://api.deepseek.com/v1", api_format:"openai", api_key_configured:true, api_key_source:"secret_store", api_key_env:"", temperature:0, max_tokens:100, invoke_timeout_ms:1000, active:false, created_at:1, updated_at:1, ...values });

describe("explicit account credentials", () => {
  it("only offers configured vault models from the same provider and API address", () => {
    const models=[model("eligible"),model("other-provider",{provider:"openai"}),model("other-endpoint",{base_url:"https://other.example/v1"}),model("environment",{api_key_source:"environment"}),model("missing",{api_key_configured:false})];
    expect(eligibleCredentialModels(models,"deepseek","https://api.deepseek.com/v1/").map(item=>item.id)).toEqual(["eligible"]);
  });
  it("does not silently reuse, requires cloud credentials, and permits local accounts", () => {
    expect(credentialChoice("", "")).toEqual({credential_action:"keep",credential_fields:{}});
    expect(credentialChoice("", "eligible")).toEqual({credential_action:"reuse_model",credential_model_id:"eligible",credential_fields:{}});
    expect(credentialChoice("new-key", "eligible")).toEqual({credential_action:"replace",credential_fields:{api_key:"new-key"}});
    expect(credentialReady(undefined,"deepseek","","")).toBe(false);
    expect(credentialReady(undefined,"ollama","","")).toBe(true);
    expect(credentialReady({id:"x",provider_id:"deepseek",base_url:"https://api.deepseek.com",label:"x",profile_ids:[],credential_id:null,credential_status:"missing",credential_configured:false},"deepseek","","")).toBe(false);
  });
  it("does not touch vision when chat verification or activation fails", async () => {
    const activate=vi.fn(async()=>true);const configureVision=vi.fn(async()=>{});
    expect(await finishQuickModelSetup({verify:async()=>null,activate,configureVision})).toBe("verify_failed");
    expect(activate).not.toHaveBeenCalled();expect(configureVision).not.toHaveBeenCalled();
    expect(await finishQuickModelSetup({verify:async()=>true,activate:async()=>null,configureVision})).toBe("activate_failed");
    expect(configureVision).not.toHaveBeenCalled();
    const calls:string[]=[];
    expect(await finishQuickModelSetup({verify:async()=>{calls.push("verify");return true},activate:async()=>{calls.push("activate");return true},configureVision:async()=>{calls.push("vision")}})).toBe("ready");
    expect(calls).toEqual(["verify","activate","vision"]);
  });
});
