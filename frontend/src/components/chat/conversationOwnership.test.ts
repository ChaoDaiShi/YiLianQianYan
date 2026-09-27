import {describe,expect,it} from 'vitest';
import {createConversationOwnership} from './conversationOwnership';
describe('conversation snapshot admission',()=>{
 it('A: admits an idle snapshot at unchanged revision',()=>{
  const owner=createConversationOwnership('A');expect(owner.admits(owner.hydration())).toBe(true);
 });
 it('B: forbids replace after a local or live mutation',()=>{
  const owner=createConversationOwnership('A'),request=owner.hydration();owner.mutate();expect(owner.admits(request)).toBe(false);
 });
 it('C/D: rejects A after switching to B and protects B live reply',()=>{
  const owner=createConversationOwnership('A'),a=owner.hydration();owner.activate('B');const b=owner.hydration();owner.mutate();expect(owner.current(a)).toBe(false);expect(owner.admits(b)).toBe(false);
 });
 it('invalidates an earlier request even within the same conversation',()=>{
  const owner=createConversationOwnership('A'),a=owner.hydration();expect(owner.admits(owner.hydration())).toBe(true);expect(owner.admits(a)).toBe(false);
 });
 it('protects a live run started before history and the initial draft promotion',()=>{
  const owner=createConversationOwnership(null);owner.start();owner.promote('A');owner.finish();expect(owner.admits(owner.hydration())).toBe(false);expect(owner.admits(owner.hydration())).toBe(true);
  owner.start();expect(owner.admits(owner.hydration())).toBe(false);
 });
 it('consumes each live completion once even after a persisted-ID refresh',()=>{
  const owner=createConversationOwnership('A');expect(owner.claimCompletion('live')).toBe(true);owner.hydration();expect(owner.claimCompletion('live')).toBe(false);owner.activate('B');expect(owner.claimCompletion('live')).toBe(true);
 });
 it('does not treat a real navigation as draft adoption',()=>{
  const owner=createConversationOwnership(null);owner.start();owner.activate('B');expect(owner.admits(owner.hydration())).toBe(true);
 });
});
