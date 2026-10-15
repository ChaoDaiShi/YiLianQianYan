import {readFile, writeFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {resolve, join} from 'node:path';
import {fileURLToPath} from 'node:url';

export async function verifyRun(row, task, root) {
  const failures=[];
  if (!row.started || row.evidence_class!=='REAL_MODEL') failures.push('not_real_model_run');
  if (row.illegal_execution_count !== 0) failures.push('execution_authorization_not_proven');
  const artifacts=new Map();
  for (const item of row.artifact_files ?? []) {
    try {
      const bytes=await readFile(resolve(root,item.path));
      if(!bytes.length) failures.push('empty_artifact');
      const digest=createHash('sha256').update(bytes).digest('hex');
      if(digest!==item.sha256) failures.push('digest_mismatch');
      if(item.mime_type==='application/json') JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
      else if(item.mime_type.startsWith('text/')) new TextDecoder('utf-8',{fatal:true}).decode(bytes);
      else failures.push('independent_format_verifier_unavailable');
      artifacts.set(item.deliverable_id,{...item,bytes});
    } catch { failures.push('artifact_unreadable_or_invalid'); }
  }
  for(const required of task.deliverables) {
    const artifact=artifacts.get(required.id);
    if(!artifact) failures.push(`missing_deliverable:${required.id}`);
    else if(artifact.mime_type!==required.mime_type) failures.push(`mime_mismatch:${required.id}`);
  }
  for(const criterion of task.criteria.filter(c=>c.required)) {
    const check=criterion.check, artifact=artifacts.get(check.deliverable_id);
    if(!artifact) continue;
    if(check.kind==='required_sections') {
      const lines=new TextDecoder().decode(artifact.bytes).split(/\r?\n/).map(l=>l.replace(/^#+\s*/, '').trim());
      for(const heading of check.headings) if(!lines.includes(heading.trim())) failures.push(`missing_heading:${heading}`);
    } else if(check.kind==='source_traceability') {
      if(!Number.isInteger(check.min_sources)||check.min_sources<1) failures.push('invalid_source_threshold');
      const text=new TextDecoder().decode(artifact.bytes);
      const sources=(row.sources??[]).filter(source=>source.cited_by_deliverable_ids?.includes(check.deliverable_id) && source.evidence_id && source.retrieved_at_ms>0 && text.includes(source.uri));
      if(new Set(sources.map(s=>s.uri)).size<check.min_sources) failures.push('insufficient_traceable_sources');
    } else if(!['artifact_exists','artifact_digest_matches'].includes(check.kind)) failures.push(`unverified_criterion:${check.kind}`);
  }
  return {pass:failures.length===0,codes:[...new Set(failures)]};
}
export async function verifyDirectory(directory, taskSpecs) {
  const rows=(await readFile(join(directory,'runs.jsonl'),'utf8')).trim().split('\n').filter(Boolean).map(JSON.parse);
  const tasks=new Map(taskSpecs.map(t=>[t.task_id,t]));
  const ids=new Set();
  for(const row of rows) {
    const key=`${row.task_id}:${row.group}:${row.repeat}`;
    if(ids.has(key)) throw new Error(`duplicate_run_slot:${key}`);ids.add(key);
    const task=tasks.get(row.task_id);if(!task) throw new Error(`unknown_task:${row.task_id}`);
    const result=await verifyRun(row,task,directory);row.judge_pass=result.pass;row.judge_codes=result.codes;
    if(row.started && row.status!=='CANCELLED' && row.status!=='BLOCKED_ENV') row.status=result.pass?'PASS':'FAIL';
  }
  await writeFile(join(directory,'judged-runs.jsonl'),rows.map(r=>JSON.stringify(r)).join('\n')+'\n');
  return rows;
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
  const i=process.argv.indexOf('--input');if(i<0) throw new Error('--input required');
  const directory=resolve(process.argv[i+1]);const manifest=JSON.parse(await readFile(join(directory,'manifest.json'),'utf8'));
  const tasks=JSON.parse(await readFile(resolve(manifest.source_task_file),'utf8')).tasks;
  const rows=await verifyDirectory(directory,tasks);
  process.stdout.write(JSON.stringify({runs:rows.length,pass:rows.filter(r=>r.judge_pass).length})+'\n');
  if(rows.some(r=>r.started&&!r.judge_pass)) process.exitCode=1;
}
