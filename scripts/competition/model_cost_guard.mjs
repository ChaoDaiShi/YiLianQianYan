export class ModelCostGuard {
  constructor({maxCny,maxRequests,maxOutputTokens}) {
    if(!Number.isFinite(maxCny)||maxCny<=0||maxCny>0.30||!Number.isInteger(maxRequests)||maxRequests<1||maxRequests>10||!Number.isInteger(maxOutputTokens)||maxOutputTokens<1||maxOutputTokens>512) throw new Error('Invalid test budget configuration');
    this.maxMicroCny = Math.floor(maxCny * 1_000_000);
    this.maxRequests = maxRequests;
    this.maxOutputTokens = maxOutputTokens;
    this.requests = 0;
    this.reservedMicroCny = 0;
  }
  reserve(body) {
    if(body.model !== 'deepseek-flash') throw new Error('Unpriced model is outside this test');
    if(this.requests >= this.maxRequests) throw new Error('Model request budget exhausted');
    const inputBytes = Buffer.byteLength(JSON.stringify({messages:body.messages,tools:body.tools,tool_choice:body.tool_choice}));
    // Byte count plus explicit framing allowance deliberately overestimates
    // normal text tokenization; use the verified peak CNY rates, without cache discounts.
    const inputTokenBound = inputBytes + ((body.messages?.length??0)+(body.tools?.length??0)+1)*128 + 1024;
    const maxOutputTokens = Math.min(body.max_tokens??this.maxOutputTokens,this.maxOutputTokens);
    const reserveMicroCny = Math.ceil(inputTokenBound*2 + maxOutputTokens*8);
    if(this.reservedMicroCny + reserveMicroCny > this.maxMicroCny) throw new Error('Model money budget exhausted');
    this.requests++;
    this.reservedMicroCny += reserveMicroCny;
    return {request:this.requests,input_bytes:inputBytes,input_token_bound:inputTokenBound,max_output_tokens:maxOutputTokens,reserved_cny:reserveMicroCny/1_000_000,cumulative_reserved_cny:this.reservedMicroCny/1_000_000};
  }
}
