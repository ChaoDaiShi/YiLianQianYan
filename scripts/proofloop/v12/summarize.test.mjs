import test from 'node:test';import assert from 'node:assert/strict';
import {summarize,completionRate,falsePassRate} from './summarize.mjs';
const manifest={task_count:30,runs_per_group:3,groups:{'V12-B0':{},'V12-B1':{},'V12-B2':{}}};
test('unrun and duplicate slots cannot become successful experiments',()=>{
  assert.equal(summarize([],manifest).status,'ACCEPTANCE_INCOMPLETE');assert.equal(completionRate([],90),0);assert.equal(falsePassRate([]),null);
  const row={task_id:'A01',group:'V12-B0',repeat:1};assert.throws(()=>summarize([row,row],manifest),/duplicate_run_slot/);
});
test('product claims require independently verified outcomes',()=>{
  assert.equal(falsePassRate([{product_claimed_pass:true,judge_pass:false}]),1);
  assert.equal(completionRate([{started:true,status:'PASS',judge_pass:true}],90),1/90);
});
