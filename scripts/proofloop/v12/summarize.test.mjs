import test from 'node:test';import assert from 'node:assert/strict';
import {summarize,completionRate,falsePassRate} from './summarize.mjs';
import {makeSchedule} from './run.mjs';
const manifest={task_count:30,runs_per_group:3,groups:{'V12-B0':{},'V12-B1':{},'V12-B2':{}}};
test('unrun and duplicate slots cannot become successful experiments',()=>{
  assert.equal(summarize([],manifest).status,'ACCEPTANCE_INCOMPLETE');assert.equal(completionRate([],90),0);assert.equal(falsePassRate([]),null);
  const row={run_id:'one',task_id:'A01',group:'V12-B0',repeat:1,status:'NOT_TESTED'};const duplicateSlot={...row,run_id:'two'};assert.throws(()=>summarize([row,duplicateSlot],manifest),/duplicate_run_slot/);
  const pilotManifest={...manifest,task_count:30,runs_per_group:3,seed:20261016,pilot_task_ids:['A01','T01','P01']};
  const schedule=makeSchedule('pilot',[{task_id:'A01',category:'analysis'},{task_id:'T01',category:'typed_transfer'},{task_id:'P01',category:'planning'}],pilotManifest);
  const pilot=summarize([],pilotManifest,schedule,{publishable_g4_identity:false});
  assert.equal(schedule.planned_slots,9);assert.equal(pilot.expected,9);assert.equal(pilot.missing_slots.length,9);assert.equal(pilot.status,'ACCEPTANCE_INCOMPLETE');
});
test('product claims require independently verified outcomes',()=>{
  assert.equal(falsePassRate([{product_claimed_pass:true,judge_pass:false}]),1);
  assert.equal(completionRate([{started:true,evidence_class:'REAL_MODEL',status:'PASS',judge_pass:true}],90),1/90);
});
