/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

use std::{future::Future, sync::Arc};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

pub(super) struct PaginationGate(Arc<Semaphore>);

impl Default for PaginationGate {
    fn default() -> Self {
        Self(Arc::new(Semaphore::new(1)))
    }
}

impl PaginationGate {
    pub(super) fn admit(&self) -> Result<OwnedSemaphorePermit, String> {
        self.0
            .clone()
            .try_acquire_owned()
            .map_err(|_error| String::from("Timeline pagination is already running"))
    }
}

// The task, rather than the IPC awaiter, owns the permit. Dropping an IPC future
// must not admit another operation while SDK-owned network work is still running.
pub(super) fn spawn_guarded<T: Send + 'static>(
    permit: OwnedSemaphorePermit,
    work: impl Future<Output = T> + Send + 'static,
) -> tokio::task::JoinHandle<T> {
    tokio::spawn(async move {
        let result = work.await;
        drop(permit);
        result
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn detached_work_retains_guard_until_it_settles() {
        let gate = PaginationGate::default();
        let (finish, pending) = tokio::sync::oneshot::channel::<()>();
        let task = spawn_guarded(gate.admit().expect("first operation"), async move {
            pending.await.unwrap();
        });
        drop(task);
        assert!(gate.admit().is_err());
        finish.send(()).unwrap();
        for _attempt in 0..100 {
            tokio::task::yield_now().await;
            if gate.admit().is_ok() {
                return;
            }
        }
        panic!("settled operation retained its guard");
    }

    #[tokio::test]
    async fn failures_release_guard_and_instances_are_independent() {
        let first = PaginationGate::default();
        let other = PaginationGate::default();
        let (finish, pending) = tokio::sync::oneshot::channel::<()>();
        let task = spawn_guarded(first.admit().unwrap(), async move {
            pending.await.unwrap();
            Err::<(), &str>("network")
        });
        assert!(first.admit().is_err());
        assert!(other.admit().is_ok());
        finish.send(()).unwrap();
        let result = task.await.unwrap();
        assert!(result.is_err());
        assert!(first.admit().is_ok());
    }
}
