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

use eyeball_im::{Vector, VectorDiff};

use crate::shell::{
    service::{emit_shell_room_updated, emit_shell_timeline_updated},
    types::{RoomTimeline, RoomTimelineIdentity, apply_timeline_presentation},
};
use futures_util::StreamExt;
use matrix_sdk::Room;
use matrix_sdk_ui::timeline::{Timeline, TimelineItem};
use std::{
    ops::Deref,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::async_runtime::JoinHandle;

pub(super) struct OrderedProjection<T: Clone> {
    pub(super) items: Vector<T>,
    pub(super) revision: u64,
}

impl<T: Clone> OrderedProjection<T> {
    pub(super) fn new(items: Vector<T>) -> Self {
        Self { items, revision: 0 }
    }

    pub(super) fn apply(&mut self, diffs: Vec<VectorDiff<T>>) {
        if diffs.is_empty() {
            return;
        }
        for diff in diffs {
            diff.apply(&mut self.items);
        }
        self.revision = self
            .revision
            .checked_add(1)
            .expect("timeline revision exhausted");
    }
}

// Instance IDs never repeat during the process, including account teardown and room recreation.
static NEXT_TIMELINE_INSTANCE: AtomicU64 = AtomicU64::new(1);

#[derive(Default)]
pub(super) struct ActivePublications {
    instances: std::collections::HashMap<String, (u64, Option<RoomTimelineIdentity>)>,
    next_generation: u64,
}

impl ActivePublications {
    pub(super) fn reserve(&mut self, account_key: &str) -> u64 {
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .expect("timeline view generation exhausted");
        self.instances
            .insert(account_key.to_owned(), (self.next_generation, None));
        self.next_generation
    }

    fn activate(&mut self, identity: &RoomTimelineIdentity, generation: u64) -> bool {
        let Some((current_generation, current_identity)) =
            self.instances.get_mut(&identity.account_key)
        else {
            return false;
        };
        if *current_generation != generation {
            return false;
        }
        *current_identity = Some(identity.clone());
        true
    }

    fn accepts(&self, identity: &RoomTimelineIdentity) -> bool {
        self.instances
            .get(&identity.account_key)
            .and_then(|(_generation, identity)| identity.as_ref())
            == Some(identity)
    }

    pub(super) fn close_account(&mut self, account_key: &str) {
        self.instances.remove(account_key);
    }

    pub(super) fn close_all(&mut self) {
        self.instances.clear();
    }
}

struct SubscriptionState {
    projection: OrderedProjection<Arc<TimelineItem>>,
    identity: RoomTimelineIdentity,
    app: Option<tauri::AppHandle>,
    valid: bool,
    publications: Arc<Mutex<ActivePublications>>,
}

pub struct TimelineInstance {
    timeline: Arc<Timeline>,
    state: Arc<Mutex<SubscriptionState>>,
    subscription: JoinHandle<()>,
}

impl TimelineInstance {
    pub(super) async fn new(
        timeline: Arc<Timeline>,
        account_key: &str,
        room: &Room,
        focused_event_id: Option<String>,
        publications: Arc<Mutex<ActivePublications>>,
    ) -> Self {
        let (items, mut stream) = timeline.subscribe().await;
        let state = Arc::new(Mutex::new(SubscriptionState {
            projection: OrderedProjection::new(items),
            identity: RoomTimelineIdentity {
                account_key: account_key.to_owned(),
                room_id: room.room_id().to_string(),
                instance_id: NEXT_TIMELINE_INSTANCE
                    .fetch_add(1, Ordering::Relaxed)
                    .to_string(),
                focused_event_id,
            },
            app: None,
            valid: true,
            publications,
        }));
        let task_state = state.clone();
        let subscription = tauri::async_runtime::spawn(async move {
            while let Some(diffs) = stream.next().await {
                if diffs.is_empty() {
                    continue;
                }
                // Applying the entire SDK batch and publishing share the invalidation lock.
                // A removed instance cannot emit after invalidate() returns.
                let mut state = task_state
                    .lock()
                    .expect("timeline subscription lock poisoned");
                if !state.valid {
                    break;
                }
                state.projection.apply(diffs);
                state.publish();
            }
        });
        Self {
            timeline,
            state,
            subscription,
        }
    }

    pub(super) fn publish_to(&self, app: tauri::AppHandle) {
        let mut state = self
            .state
            .lock()
            .expect("timeline subscription lock poisoned");
        if !state.valid {
            return;
        }
        state.app = Some(app);
    }

    pub(super) fn activate(&self, generation: u64) -> bool {
        let state = self
            .state
            .lock()
            .expect("timeline subscription lock poisoned");
        state.valid
            && state
                .publications
                .lock()
                .expect("timeline publication lock poisoned")
                .activate(&state.identity, generation)
    }

    pub(in crate::shell) fn snapshot(
        &self,
        next_before: Option<String>,
    ) -> Result<RoomTimeline, String> {
        let state = self
            .state
            .lock()
            .expect("timeline subscription lock poisoned");
        if !state.valid {
            return Err(String::from("Timeline instance is no longer active"));
        }
        Ok(state.snapshot(next_before))
    }

    pub(super) fn invalidate(&self) {
        let mut state = self
            .state
            .lock()
            .expect("timeline subscription lock poisoned");
        state.valid = false;
        state.app = None;
        drop(state);
        self.subscription.abort();
    }
}

impl SubscriptionState {
    fn snapshot(&self, next_before: Option<String>) -> RoomTimeline {
        let mut items = super::timeline_items_to_shell_items(
            self.projection.items.iter(),
            &self.identity.room_id,
        );
        apply_timeline_presentation(&mut items, &self.identity.room_id);
        RoomTimeline {
            timeline_identity: self.identity.clone(),
            revision: self.projection.revision,
            room_id: self.identity.room_id.clone(),
            items,
            next_before,
            focused_event_id: self.identity.focused_event_id.clone(),
            redacted_event_ids: super::redacted_event_ids_from_timeline_items(
                self.projection.items.iter(),
            ),
        }
    }

    #[allow(
        clippy::significant_drop_tightening,
        reason = "The publication lock must remain held through emission to serialize view invalidation"
    )]
    fn publish(&self) {
        // Hold the publication gate through emission so closing/replacing a view
        // synchronizes with any publication already in progress.
        let publications = self
            .publications
            .lock()
            .expect("timeline publication lock poisoned");
        if !publications.accepts(&self.identity) {
            return;
        }
        if let Some(app) = &self.app {
            emit_shell_timeline_updated(app, self.snapshot(None));
            emit_shell_room_updated(
                app,
                &self.identity.account_key,
                &self.identity.room_id,
                false,
            );
        }
    }
}

impl Deref for TimelineInstance {
    type Target = Timeline;
    fn deref(&self) -> &Timeline {
        &self.timeline
    }
}

impl Drop for TimelineInstance {
    fn drop(&mut self) {
        self.invalidate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeball_im::{Vector, VectorDiff};

    fn select(publications: &mut ActivePublications, identity: &RoomTimelineIdentity) {
        let generation = publications.reserve(&identity.account_key);
        assert!(publications.activate(identity, generation));
    }

    #[test]
    fn late_timeline_creation_cannot_reopen_a_replaced_or_closed_view() {
        let mut publications = ActivePublications::default();
        let identity = RoomTimelineIdentity {
            account_key: "account".into(),
            room_id: "A".into(),
            instance_id: "1".into(),
            focused_event_id: None,
        };
        let stale_generation = publications.reserve("account");
        let current_generation = publications.reserve("account");
        assert!(!publications.activate(&identity, stale_generation));
        assert!(publications.activate(&identity, current_generation));
        publications.close_all();
        assert!(!publications.activate(&identity, current_generation));
    }

    #[test]
    fn snapshot_revision_only_advances_when_subscription_batch_is_consumed() {
        let mut state = SubscriptionState {
            projection: OrderedProjection::new(Vector::new()),
            identity: RoomTimelineIdentity {
                account_key: "account".into(),
                room_id: "room".into(),
                instance_id: "instance".into(),
                focused_event_id: Some("$anchor".into()),
            },
            app: None,
            valid: true,
            publications: Arc::default(),
        };
        let before = state.snapshot(Some("cursor".into()));
        // Completion/status does not manufacture a subscription revision.
        let after_completion = state.snapshot(None);
        assert_eq!(before.revision, after_completion.revision);
        state.projection.apply(vec![VectorDiff::Clear]);
        let after_subscription = state.snapshot(None);
        assert_eq!(after_subscription.revision, before.revision + 1);
        assert_eq!(
            after_subscription.timeline_identity,
            before.timeline_identity
        );
        assert_eq!(
            after_subscription.focused_event_id.as_deref(),
            Some("$anchor")
        );
        assert!(after_subscription.items.is_empty());
    }

    #[test]
    fn publication_gate_isolates_accounts_and_room_a_b_a() {
        let mut publications = ActivePublications::default();
        let room_a = RoomTimelineIdentity {
            account_key: "account".into(),
            room_id: "A".into(),
            instance_id: "1".into(),
            focused_event_id: None,
        };
        let mut room_b = room_a.clone();
        room_b.room_id = "B".into();
        room_b.instance_id = "2".into();
        select(&mut publications, &room_a);
        select(&mut publications, &room_b);
        assert!(!publications.accepts(&room_a));
        select(&mut publications, &room_a);
        assert!(!publications.accepts(&room_b));
        assert!(publications.accepts(&room_a));
        let mut other_account = room_a.clone();
        other_account.account_key = "other".into();
        assert!(!publications.accepts(&other_account));
        publications.close_account("account");
        assert!(!publications.accepts(&room_a));
    }

    #[test]
    fn publication_gate_rejects_replaced_context_and_closed_account() {
        let mut publications = ActivePublications::default();
        let live = RoomTimelineIdentity {
            account_key: "account".into(),
            room_id: "room".into(),
            instance_id: "1".into(),
            focused_event_id: None,
        };
        let mut focused = live.clone();
        focused.instance_id = "2".into();
        focused.focused_event_id = Some("$anchor".into());
        select(&mut publications, &live);
        assert!(publications.accepts(&live));
        select(&mut publications, &focused);
        assert!(!publications.accepts(&live));
        assert!(publications.accepts(&focused));
        publications.close_account("account");
        assert!(!publications.accepts(&focused));
        let mut recreated = live.clone();
        recreated.instance_id = "3".into();
        select(&mut publications, &recreated);
        assert!(!publications.accepts(&live));
        assert!(publications.accepts(&recreated));
    }

    #[test]
    fn subscription_projection_preserves_initial_items_and_batch_order() {
        let mut projection = OrderedProjection::new(Vector::from(vec!["echo", "latest"]));
        assert_eq!(projection.revision, 0);
        projection.apply(vec![
            VectorDiff::PushFront { value: "older" },
            VectorDiff::Set {
                index: 1,
                value: "sent",
            },
        ]);
        assert_eq!(
            projection.items,
            Vector::from(vec!["older", "sent", "latest"])
        );
        assert_eq!(projection.revision, 1);
        projection.apply(vec![VectorDiff::Remove { index: 1 }]);
        assert_eq!(projection.items, Vector::from(vec!["older", "latest"]));
        assert_eq!(projection.revision, 2);
    }

    #[test]
    fn reset_and_identical_messages_follow_sdk_membership() {
        let mut projection = OrderedProjection::new(Vector::from(vec!["same", "same"]));
        projection.apply(vec![VectorDiff::Reset {
            values: Vector::from(vec!["edited"]),
        }]);
        assert_eq!(projection.items, Vector::from(vec!["edited"]));
        projection.apply(vec![]);
        assert_eq!(projection.revision, 1);
    }
}
