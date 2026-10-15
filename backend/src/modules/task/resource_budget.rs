//! Shared reservations for parent/child model calls; unknown usage remains unknown.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct BudgetReservation { pub handoff_id:String, pub calls:u32, pub tokens:u64 }
#[derive(Debug, Default)]
struct Usage { calls:u32, tokens:u64, unknown:bool, reservations:HashMap<String,(u32,u64)> }
#[derive(Debug)]
pub struct SharedExecutionBudget { max_calls:u32, max_tokens:u64, usage:Mutex<Usage> }
impl SharedExecutionBudget {
    pub fn new(max_calls:u32,max_tokens:u64)->Arc<Self> { Arc::new(Self{max_calls,max_tokens,usage:Mutex::new(Usage::default())}) }
    pub fn reserve(self:&Arc<Self>,request:BudgetReservation)->Result<BudgetLease,String> {
        let mut usage=self.usage.lock().map_err(|_|"资源预算不可用")?;
        if usage.unknown { return Err("proofloop_usage_unavailable".into()); }
        if usage.reservations.contains_key(&request.handoff_id) { return Err("重复的任务交接标识".into()); }
        let reserved_calls:u64=usage.reservations.values().map(|v|u64::from(v.0)).sum();
        let reserved_tokens:u128=usage.reservations.values().map(|v|u128::from(v.1)).sum();
        if reserved_calls+u64::from(usage.calls)+u64::from(request.calls)>u64::from(self.max_calls)
            || reserved_tokens+u128::from(usage.tokens)+u128::from(request.tokens)>u128::from(self.max_tokens) { return Err("proofloop_budget_exhausted".into()); }
        usage.reservations.insert(request.handoff_id.clone(),(request.calls,request.tokens));
        Ok(BudgetLease{budget:Arc::clone(self),handoff_id:request.handoff_id})
    }
    pub fn begin_call(&self,handoff:Option<&str>)->Result<(),String> {
        let mut usage=self.usage.lock().map_err(|_|"资源预算不可用")?;
        if usage.unknown { return Err("proofloop_usage_unavailable".into()); }
        if let Some(id)=handoff {
            let remaining=usage.reservations.get_mut(id).ok_or("任务预算未预留")?;
            if remaining.0==0 { return Err("proofloop_budget_exhausted".into()); }remaining.0-=1;
        } else {
            let reserved:u64=usage.reservations.values().map(|v|u64::from(v.0)).sum();
            if u64::from(usage.calls)+reserved>=u64::from(self.max_calls) { return Err("proofloop_budget_exhausted".into()); }
        }
        usage.calls+=1;Ok(())
    }
    pub fn settle_tokens(&self,handoff:Option<&str>,tokens:Option<u64>)->Result<(),String> {
        let mut usage=self.usage.lock().map_err(|_|"资源预算不可用")?;
        let Some(tokens)=tokens else {usage.unknown=true;return Err("proofloop_usage_unavailable".into());};
        if let Some(id)=handoff {
            let remaining=usage.reservations.get_mut(id).ok_or("任务预算未预留")?;
            if tokens>remaining.1 {usage.unknown=true;return Err("proofloop_budget_exhausted".into());}remaining.1-=tokens;
        }
        usage.tokens=usage.tokens.checked_add(tokens).ok_or("预算计数超出范围")?;
        if usage.tokens>self.max_tokens {return Err("proofloop_budget_exhausted".into());}Ok(())
    }
    pub fn usage(&self)->Result<(u32,Option<u64>),String> {
        let u=self.usage.lock().map_err(|_|"资源预算不可用")?;Ok((u.calls,if u.unknown{None}else{Some(u.tokens)}))
    }
}
pub struct BudgetLease { budget:Arc<SharedExecutionBudget>, pub handoff_id:String }
impl Drop for BudgetLease {fn drop(&mut self){if let Ok(mut usage)=self.budget.usage.lock(){usage.reservations.remove(&self.handoff_id);}}}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concurrent_reservations_cannot_spend_same_tokens() {
        let budget=SharedExecutionBudget::new(6,1000);
        let first=budget.reserve(BudgetReservation{handoff_id:"a".into(),calls:2,tokens:800}).unwrap();
        assert!(budget.reserve(BudgetReservation{handoff_id:"b".into(),calls:2,tokens:800}).is_err());
        drop(first);assert!(budget.reserve(BudgetReservation{handoff_id:"b".into(),calls:2,tokens:800}).is_ok());
    }
    #[test]
    fn missing_usage_blocks_further_budgeted_calls() {
        let budget=SharedExecutionBudget::new(6,1000);budget.begin_call(None).unwrap();
        assert!(budget.settle_tokens(None,None).is_err());assert!(budget.begin_call(None).is_err());
        assert_eq!(budget.usage().unwrap(),(1,None));
    }
}
