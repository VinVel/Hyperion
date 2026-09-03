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

import type { RoomTimelineItem } from "../appShellAdapters";

export type TimelineInfoPresentation = {
  fields: Array<[string, string]>;
  rawEventIsVisibleByDefault: false;
  rawEventJson: string | null;
  receipts: Array<[string, string]>;
  threadIndicator: { disabled: true; label: string } | null;
};

export const timelineInfoViewLabels = ["Easy View", "Advanced View"] as const;

export function timelineInfoPresentation(
  item: RoomTimelineItem,
  rawEventJson: string | null = null,
): TimelineInfoPresentation {
  const fields: Array<[string, string]> = [
    ["Event ID", item.id],
    ["Room ID", item.roomId ?? "Unavailable"],
    ["Sender", item.senderDisplayName || item.senderId],
    ["Sender ID", item.senderId],
    ["Timestamp", String(item.timestampUnixMs)],
    ["Send state", item.sendState],
    ["Decryption", item.decryptionState],
    ["Redacted", item.isRedacted ? "Yes" : "No"],
    ["Edited", item.isEdited ? "Yes" : "No"],
  ];
  if (item.transactionId) fields.push(["Transaction ID", item.transactionId]);
  if (item.replyPreview) fields.push(["Reply to", item.replyPreview.eventId]);
  if (item.threadReplyTo)
    fields.push(["Thread reply to", item.threadReplyTo.rootEventId]);
  if (item.thread?.latestEventId)
    fields.push(["Latest thread event", item.thread.latestEventId]);

  return {
    fields,
    rawEventIsVisibleByDefault: false,
    rawEventJson,
    receipts: item.receipts.map((receipt) => [
      receipt.displayName || receipt.userId,
      receipt.timestampUnixMs === null
        ? "No timestamp"
        : String(receipt.timestampUnixMs),
    ]),
    threadIndicator: item.thread
      ? { label: `${item.thread.replyCount} thread replies`, disabled: true }
      : null,
  };
}

export function formatRawEventJson(rawEventJson: string): string {
  try {
    return JSON.stringify(JSON.parse(rawEventJson), null, 2);
  } catch {
    return rawEventJson;
  }
}
