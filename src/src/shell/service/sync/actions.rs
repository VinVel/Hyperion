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

use matrix_sdk::{
    Room,
    ruma::{
        OwnedEventId,
        events::{
            AnyMessageLikeEventContent, room::message::RoomMessageEventContentWithoutRelation,
        },
    },
};

use super::{coordinator::ShellSyncCoordinator, diagnostics::emit_timeline_room_diagnostic};

impl ShellSyncCoordinator {
    pub(in crate::shell::service) async fn send_live_message(
        &self,
        account_key: &str,
        room: &Room,
        content: AnyMessageLikeEventContent,
    ) -> Result<String, String> {
        emit_timeline_room_diagnostic("timeline.live.send", account_key, room);
        self.timeline_service
            .registry()
            .send_live_message(account_key, room, content)
            .await
    }
    pub(in crate::shell::service) async fn edit_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        content: RoomMessageEventContentWithoutRelation,
    ) -> Result<(), String> {
        emit_timeline_room_diagnostic("timeline.live.edit", account_key, room);
        self.timeline_service
            .registry()
            .edit_live_message(account_key, room, event_id, content)
            .await
    }
    pub(in crate::shell::service) async fn redact_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        reason: Option<&str>,
    ) -> Result<(), String> {
        emit_timeline_room_diagnostic("timeline.live.redact", account_key, room);
        self.timeline_service
            .registry()
            .redact_live_message(account_key, room, event_id, reason)
            .await
    }
    pub(in crate::shell::service) async fn reply_to_live_message(
        &self,
        account_key: &str,
        room: &Room,
        event_id: OwnedEventId,
        content: RoomMessageEventContentWithoutRelation,
    ) -> Result<(), String> {
        emit_timeline_room_diagnostic("timeline.live.reply", account_key, room);
        self.timeline_service
            .registry()
            .reply_to_live_message(account_key, room, event_id, content)
            .await
    }
    pub(in crate::shell::service) async fn toggle_live_reaction(
        &self,
        account_key: &str,
        room: &Room,
        event_id: &str,
        reaction_key: &str,
    ) -> Result<bool, String> {
        emit_timeline_room_diagnostic("timeline.live.reaction", account_key, room);
        self.timeline_service
            .registry()
            .toggle_live_reaction(account_key, room, event_id, reaction_key)
            .await
    }
}
