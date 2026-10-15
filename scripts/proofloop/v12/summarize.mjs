import {readFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {fileURLToPath} from 'node:url';
export function completionRate(rows,scheduled){return scheduled?rows.filter(r=>r.started&&r.judge_pass===true&&r.status==='PASS').length/scheduled:null;}
export function falsePassRate(rows){const claimed=rows.filter(r=>r.product_claimed_pass===true);return claimed.length?claimed.filter(r=>r.judge_pass!==true).length/claimed.length:null;}
export function summarize(rows,manifest){
  const seen=new Set();for(const row of rows){const key=`${row.task_id}:${row.group}:${row.repeat}`;if(seen.has(key))throw new Error(`duplicate_run_slot:${key}`);seen.add(key);}
  const groups={};
  for(const group of Object.keys(manifest.groups)){
    const values=rows.filter(r=>r.group===group),started=values.filter(r=>r.started),scheduled=manifest.task_count*manifest.runs_per_group;
    const numeric=field=>values.map(r=>r[field]).filter(Number.isFinite);
    groups[group]={scheduled,recorded:values.length,started:started.length,coverage:started.length/scheduled,completion_rate:completionRate(values,scheduled),false_pass_rate:falsePassRate(values),initial_compile_ready_rate:started.length?started.filter(r=>r.initial_compile_ready===true).length/started.length:null,illegal_execution_count:values.every(r=>Number.isInteger(r.illegal_execution_count))?values.reduce((s,r)=>s+r.illegal_execution_count,0):null,total_tokens_known:numeric('total_tokens').reduce((a,b)=>a+b,0),token_usage_unknown:values.filter(r=>r.total_tokens==null).length,model_calls_known:numeric('model_calls').reduce((a,b)=>a+b,0)};
  }
  const expected=manifest.task_count*manifest.runs_per_group*Object.keys(manifest.groups).length;
  const complete=rows.length===expected&&rows.every(r=>r.started&&r.evidence_class==='REAL_MODEL')&&Object.values(groups).every(g=>g.recorded===g.scheduled);
  const base=groups['V12-B0']?.completion_rate,full=groups['V12-B2']?.completion_rate;
  return {benchmark_id:manifest.benchmark_id,status:complete?'MEASURED':'ACCEPTANCE_INCOMPLETE',expected,recorded:rows.length,groups,relative_lift:base>0?(full-base)/base:null,percentage_point_change:Number.isFinite(base)&&Number.isFinite(full)?100*(full-base):null};
}
if(process.argv[1]&&resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const i=process.argv.indexOf('--input');if(i<0)throw new Error('--input required');const directory=resolve(process.argv[i+1]);
  const rows=(await readFile(join(directory,'judged-runs.jsonl'),'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);
  const manifest=JSON.parse(await readFile(join(directory,'manifest.json'),'utf8'));const result=summarize(rows,manifest);
  await writeFile(join(directory,'summary.json'),JSON.stringify(result,null,2));
  await writeFile(join(directory,'report.md'),`# ${result.status}\n\n真实模型运行记录：${result.recorded}/${result.expected}。\n\n\`\`\`json\n${JSON.stringify(result,null,2)}\n\`\`\`\n`);
  process.stdout.write(JSON.stringify(result)+'\n');if(result.status!=='MEASURED')process.exitCode=2;
}
