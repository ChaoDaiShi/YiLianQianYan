import { createServer } from 'node:http';
import { ModelCostGuard } from './model_cost_guard.mjs';

// Transport-only relay: responses and tool choices come from the real provider.
// Authorization stays in process memory and is never included in receipts.
export async function startModelBudgetProxy({upstreamBase,maxCny=0.30,maxRequests=10,maxOutputTokens=512}) {
  const upstream = new URL(upstreamBase);
  if(upstream.origin !== 'https://api.deepseek.com') throw new Error('This price book only covers the official DeepSeek endpoint');
  const guard = new ModelCostGuard({maxCny,maxRequests,maxOutputTokens});
  const receipt = {model:'deepseek-flash',thinking:'disabled',max_cny:maxCny,max_requests:maxRequests,max_output_tokens:maxOutputTokens,price_cny_per_million:{input_miss:2,output:8},price_source:'https://api-docs.deepseek.com/zh-cn/quick_start/pricing/',reservation_method:'utf8_input_bytes_plus_framing_allowance',requests:[],denied:[]};
  let authorization,active=0;
  async function balance() {
    const response = await fetch(new URL('/user/balance',upstream),{headers:{Authorization:authorization},signal:AbortSignal.timeout(10000)});
    if(!response.ok) throw new Error('Provider balance endpoint returned '+response.status);
    const payload = await response.json();
    const cny = payload.balance_infos?.find(item=>item.currency==='CNY');
    if(!cny||!Number.isFinite(Number(cny.total_balance))) throw new Error('A CNY balance is required for this test budget');
    return {at:new Date().toISOString(),currency:'CNY',total_balance:Number(cny.total_balance),is_available:payload.is_available};
  }
  const server = createServer(async(request,response)=>{
    try {
      if(request.method!=='POST'||request.url!=='/v1/chat/completions') { response.writeHead(404).end();return; }
      if(active!==0) throw new Error('Only one model request may be in flight');
      active++;
      let raw='';for await(const chunk of request)raw+=chunk;
      const body=JSON.parse(raw);
      authorization=request.headers.authorization;
      if(!receipt.balance_before){receipt.balance_before=await balance();if(!receipt.balance_before.is_available||receipt.balance_before.total_balance<maxCny)throw new Error('Insufficient balance to reserve this bounded test');}
      const entry={at:new Date().toISOString(),...guard.reserve(body)};
      receipt.requests.push(entry);
      body.max_tokens=entry.max_output_tokens;
      body.thinking={type:'disabled'};
      const result=await fetch(upstreamBase.replace(/\/$/,'')+'/chat/completions',{method:'POST',headers:{'Content-Type':'application/json',Authorization:authorization},body:JSON.stringify(body),signal:AbortSignal.timeout(60000)});
      entry.http_status=result.status;
      response.writeHead(result.status,{'Content-Type':result.headers.get('content-type')??'text/event-stream','Cache-Control':'no-cache'});
      const decoder=new TextDecoder();let buffer='';
      for await(const chunk of result.body){response.write(chunk);buffer+=decoder.decode(chunk,{stream:true});let newline;while((newline=buffer.indexOf('\n'))>=0){const line=buffer.slice(0,newline).trim();buffer=buffer.slice(newline+1);if(!line.startsWith('data:'))continue;const data=line.slice(5).trim();if(!data||data==='[DONE]')continue;const value=JSON.parse(data);if(value.usage)entry.usage=value.usage;}}
      response.end();
    } catch(error) {
      receipt.denied.push({at:new Date().toISOString(),reason:String(error)});
      if(!response.headersSent)response.writeHead(429,{'Content-Type':'application/json'});
      response.end(JSON.stringify({error:{message:'Bounded test stopped: '+String(error)}}));
    } finally {active=Math.max(0,active-1);}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  return {
    baseUrl:`http://127.0.0.1:${server.address().port}/v1`,
    async finish(){
      if(authorization){try{receipt.balance_after=await balance();receipt.balance_delta_cny=Number((receipt.balance_before.total_balance-receipt.balance_after.total_balance).toFixed(8));}catch(error){receipt.balance_after_error=String(error);}}
      receipt.cumulative_reserved_cny=guard.reservedMicroCny/1_000_000;
      authorization=undefined;
      await new Promise(resolve=>server.close(resolve));
      return receipt;
    }
  };
}
