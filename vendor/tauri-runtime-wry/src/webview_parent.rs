// Copyright 2019-2026 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use std::{
  ops::Deref,
  sync::{LockResult, Mutex, MutexGuard},
};

use super::{Result, WindowId};

/// Parent identity is shared with native callbacks and reconstructed IPC dispatchers.
#[derive(Debug)]
pub(super) struct WebviewParent {
  id: Mutex<WindowId>,
  dispatch: Mutex<()>,
}

/// The temporary enqueue guard preserves the previous dispatcher lock lifetime.
pub(super) struct DispatchId<'a> {
  id: MutexGuard<'a, WindowId>,
  _dispatch: MutexGuard<'a, ()>,
}

impl Deref for DispatchId<'_> {
  type Target = WindowId;

  fn deref(&self) -> &Self::Target {
    &self.id
  }
}

impl WebviewParent {
  pub(super) fn new(id: WindowId) -> Self {
    Self {
      id: Mutex::new(id),
      dispatch: Mutex::new(()),
    }
  }

  pub(super) fn lock(&self) -> LockResult<MutexGuard<'_, WindowId>> {
    self.id.lock()
  }

  pub(super) fn dispatch_id(&self) -> DispatchId<'_> {
    let dispatch = self.dispatch.lock().unwrap();
    DispatchId {
      id: self.id.lock().unwrap(),
      _dispatch: dispatch,
    }
  }

  pub(super) fn reparent(
    &self,
    destination: WindowId,
    native_move: impl FnOnce(WindowId) -> Result<()>,
  ) -> Result<()> {
    let _move = self.dispatch.lock().unwrap();
    let previous = *self.id.lock().unwrap();
    // Native SetParent may synchronously call GotFocus/LostFocus on the UI thread.
    // Those callbacks read id, so no id guard may survive the UI send/receive wait.
    native_move(previous)?;
    *self.id.lock().unwrap() = destination;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::{
    sync::{mpsc, Arc},
    thread,
    time::Duration,
  };

  #[test]
  fn native_callback_can_read_parent_while_move_waits_and_failure_retains_parent() {
    let parent = Arc::new(WebviewParent::new(WindowId::from(1)));
    let callback_parent = parent.clone();
    let (tx, rx) = mpsc::channel();
    let (start_tx, start_rx) = mpsc::channel();
    let callback = thread::spawn(move || {
      start_rx.recv_timeout(Duration::from_secs(1)).unwrap();
      tx.send(*callback_parent.lock().unwrap()).unwrap();
    });
    let result = parent.reparent(WindowId::from(2), |previous| {
      assert_eq!(previous, WindowId::from(1));
      start_tx.send(()).unwrap();
      let observed = rx
        .recv_timeout(Duration::from_secs(1))
        .map_err(|_| crate::Error::FailedToSendMessage)?;
      assert_eq!(observed, WindowId::from(1));
      Ok(())
    });
    callback.join().unwrap();
    result.unwrap();
    assert_eq!(*parent.lock().unwrap(), WindowId::from(2));

    let error = parent.reparent(WindowId::from(3), |_| {
      Err(crate::Error::FailedToSendMessage)
    });
    assert!(error.is_err());
    assert_eq!(*parent.lock().unwrap(), WindowId::from(2));
  }

  #[test]
  fn cloned_dispatchers_serialize_moves_and_snapshot_the_committed_parent() {
    let parent = Arc::new(WebviewParent::new(WindowId::from(1)));
    let first_parent = parent.clone();
    let second_parent = parent.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let first = thread::spawn(move || {
      first_parent.reparent(WindowId::from(2), |previous| {
        entered_tx.send(previous).unwrap();
        release_rx
          .recv_timeout(Duration::from_secs(2))
          .map_err(|_| crate::Error::FailedToSendMessage)?;
        Ok(())
      })
    });
    assert_eq!(
      entered_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
      WindowId::from(1)
    );
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (second_tx, second_rx) = mpsc::channel();
    let second = thread::spawn(move || {
      attempt_tx.send(()).unwrap();
      second_parent.reparent(WindowId::from(3), |previous| {
        second_tx.send(previous).unwrap();
        Ok(())
      })
    });
    attempt_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
      second_rx.recv_timeout(Duration::from_millis(100)),
      Err(mpsc::RecvTimeoutError::Timeout)
    ));
    // A native callback is not serialized behind the outstanding move.
    assert_eq!(*parent.lock().unwrap(), WindowId::from(1));
    release_tx.send(()).unwrap();
    first.join().unwrap().unwrap();
    assert_eq!(
      second_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
      WindowId::from(2)
    );
    second.join().unwrap().unwrap();
    assert_eq!(*parent.lock().unwrap(), WindowId::from(3));
  }

  #[test]
  fn ordinary_dispatch_waits_for_move_commit_but_native_callback_does_not() {
    let parent = Arc::new(WebviewParent::new(WindowId::from(1)));
    let moving = parent.clone();
    let ordinary = parent.clone();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let move_thread = thread::spawn(move || {
      moving.reparent(WindowId::from(2), |_| {
        entered_tx.send(()).unwrap();
        release_rx
          .recv_timeout(Duration::from_secs(2))
          .map_err(|_| crate::Error::FailedToSendMessage)?;
        Ok(())
      })
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let (attempt_tx, attempt_rx) = mpsc::channel();
    let (enqueued_tx, enqueued_rx) = mpsc::channel();
    let dispatch_thread = thread::spawn(move || {
      attempt_tx.send(()).unwrap();
      enqueued_tx.send(*ordinary.dispatch_id()).unwrap();
    });
    attempt_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(matches!(
      enqueued_rx.recv_timeout(Duration::from_millis(100)),
      Err(mpsc::RecvTimeoutError::Timeout)
    ));
    assert_eq!(*parent.lock().unwrap(), WindowId::from(1));
    release_tx.send(()).unwrap();
    move_thread.join().unwrap().unwrap();
    assert_eq!(
      enqueued_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
      WindowId::from(2)
    );
    dispatch_thread.join().unwrap();
  }

  #[test]
  fn getter_enqueue_guard_is_released_before_waiting_for_reply() {
    let parent = Arc::new(WebviewParent::new(WindowId::from(1)));
    let getter_parent = parent.clone();
    let (request_tx, request_rx) = mpsc::channel();
    let (reply_tx, reply_rx) = mpsc::channel();
    let getter = thread::spawn(move || {
      // This statement boundary matches webview_getter!/getter! in lib.rs.
      request_tx.send(*getter_parent.dispatch_id()).unwrap();
      reply_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    });
    assert_eq!(
      request_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
      WindowId::from(1)
    );
    let (moved_tx, moved_rx) = mpsc::channel();
    let moving = parent.clone();
    let move_thread = thread::spawn(move || {
      moving.reparent(WindowId::from(2), |_| Ok(())).unwrap();
      moved_tx.send(()).unwrap();
    });
    moved_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    reply_tx.send(()).unwrap();
    getter.join().unwrap();
    move_thread.join().unwrap();
    assert_eq!(*parent.lock().unwrap(), WindowId::from(2));
  }
}
