import test from 'node:test';
import assert from 'node:assert/strict';
import { desktopGate } from './smoke_contract.mjs';

test('restored tabs and a foreground app alone never authorize keyboard input', () => {
  assert.equal(desktopGate({windows:[{document_matches_expected:false,visible:true,focused_document:true}]}),false);
  assert.equal(desktopGate({windows:[{document_matches_expected:true,visible:true,focused_document:false}]}),false);
  assert.equal(desktopGate({windows:[]}),false);
  assert.equal(desktopGate({windows:[{document_matches_expected:true,visible:true,focused_document:true},{document_matches_expected:false,visible:true,focused_document:false}]}),false);
  assert.equal(desktopGate({windows:[{document_matches_expected:true,visible:true,focused_document:true}]}),true);
});
