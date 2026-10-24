import test from 'node:test';
import assert from 'node:assert/strict';
import { ModelCostGuard } from './model_cost_guard.mjs';

test('reserves peak-price input bound and output cap before forwarding', () => {
  const guard = new ModelCostGuard({maxCny:0.30,maxRequests:10,maxOutputTokens:512});
  const receipt = guard.reserve({model:'deepseek-flash',messages:[{role:'user',content:'中文预算检查'}],max_tokens:8192});
  assert.equal(receipt.max_output_tokens,512);
  assert.ok(receipt.input_token_bound >= Buffer.byteLength('中文预算检查'));
  assert.ok(receipt.reserved_cny > 0);
  assert.equal(guard.requests,1);
});

test('denies excess money or requests without incrementing the reservation', () => {
  const money = new ModelCostGuard({maxCny:0.001,maxRequests:10,maxOutputTokens:512});
  assert.throws(()=>money.reserve({model:'deepseek-flash',messages:[],max_tokens:512}),/money budget/);
  assert.equal(money.requests,0);
  const calls = new ModelCostGuard({maxCny:0.30,maxRequests:1,maxOutputTokens:512});
  calls.reserve({model:'deepseek-flash',messages:[]});
  const reserved = calls.reservedMicroCny;
  assert.throws(()=>calls.reserve({model:'deepseek-flash',messages:[]}),/request budget/);
  assert.equal(calls.reservedMicroCny,reserved);
  assert.throws(()=>new ModelCostGuard({maxCny:0.30,maxRequests:10,maxOutputTokens:512}).reserve({model:'expensive-model',messages:[]}),/model/);
});

test('invalid environment budget values cannot disable the cap', () => {
  for(const maxCny of [NaN,-1,0,0.31]) assert.throws(()=>new ModelCostGuard({maxCny,maxRequests:10,maxOutputTokens:512}),/budget configuration/);
});
