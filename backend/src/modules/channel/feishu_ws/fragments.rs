use super::{Frame, HEADER_MESSAGE_ID, HEADER_SEQUENCE, HEADER_SUM, HEADER_TRACE_ID};
use std::{collections::HashMap, time::Duration};

const FRAGMENT_LIFETIME: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct FragmentAssembler {
    pending: HashMap<String, PendingFragments>,
}

struct PendingFragments {
    created_at: tokio::time::Instant,
    trace_id: String,
    parts: Vec<Option<Vec<u8>>>,
}

impl FragmentAssembler {
    pub(super) fn push(&mut self, frame: Frame) -> Result<Option<Vec<u8>>, String> {
        let message_id = frame
            .header(HEADER_MESSAGE_ID)
            .filter(|id| !id.is_empty())
            .ok_or("飞书事件帧缺少message_id")?
            .to_string();
        let sum = frame
            .header(HEADER_SUM)
            .ok_or("飞书事件帧缺少sum")?
            .parse::<usize>()
            .map_err(|_| "飞书事件分片sum无效")?;
        let sequence = frame
            .header(HEADER_SEQUENCE)
            .ok_or("飞书事件帧缺少seq")?
            .parse::<usize>()
            .map_err(|_| "飞书事件分片seq无效")?;
        if sum == 0 || sequence >= sum {
            return Err("飞书事件fragment元数据无效".into());
        }
        let trace_id = frame
            .header(HEADER_TRACE_ID)
            .unwrap_or_default()
            .to_string();
        let now = tokio::time::Instant::now();
        self.pending
            .retain(|_, entry| now.duration_since(entry.created_at) <= FRAGMENT_LIFETIME);

        self.pending
            .entry(message_id.clone())
            .or_insert_with(|| PendingFragments {
                created_at: now,
                trace_id: trace_id.clone(),
                parts: vec![None; sum],
            });
        let inconsistent = self
            .pending
            .get(&message_id)
            .is_some_and(|entry| entry.parts.len() != sum || entry.trace_id != trace_id);
        if inconsistent {
            self.pending.remove(&message_id);
            return Err("飞书事件fragment元数据不一致".into());
        }
        let entry = self
            .pending
            .get_mut(&message_id)
            .expect("fragment entry was just inserted or observed");
        entry.parts[sequence] = Some(frame.payload);
        if entry.parts.iter().any(Option::is_none) {
            return Ok(None);
        }
        let complete = self
            .pending
            .remove(&message_id)
            .expect("fragment entry was just observed")
            .parts
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        Ok(Some(complete))
    }
}
