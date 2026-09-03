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

export type AccountSummary = {
  account_key: string;
  user_id: string;
  homeserver_url: string;
  is_active: boolean;
};

export type AuthenticatedShellView = "messages" | "spaces" | "settings";

export type BackendRoomSummary = {
  room_id: string;
  title: string;
  participant_label: string;
  homeserver_label: string;
  topic?: string | null;
  is_direct: boolean;
  can_send_messages: boolean;
};

export type RoomSummary = {
  id: string;
  title: string;
  participantLabel: string;
  homeserverLabel: string;
  topic: string;
  isDirect: boolean;
  canSendMessages: boolean;
};

export type BackendRoomTimelineItem = {
  matrix: {
    event_id: string;
    transaction_id?: string | null;
    sender_id: string;
    room_id?: string | null;
    timestamp_unix_ms: number;
    is_own_message: boolean;
    content: {
      kind:
        | "text"
        | "pending_decryption"
        | "unable_to_decrypt"
        | "redacted"
        | "non_text"
        | "unsupported";
      body: string;
      formatted_body?: string | null;
      formatted_body_format?: string | null;
      rich_text?: BackendTimelineRichTextNode[] | null;
      is_edited: boolean;
      is_redacted: boolean;
    };
    send_state: "pending" | "sending" | "sent" | "failed" | "retrying";
    decryption_state:
      "unencrypted" | "decrypted" | "unable_to_decrypt" | "pending";
    reactions: BackendRoomTimelineReaction[];
    receipts: BackendRoomTimelineReceipt[];
    thread?: BackendRoomTimelineThreadRelation | null;
    thread_reply_to?: BackendRoomTimelineThreadReplyRelation | null;
    attachments: BackendRoomTimelineAttachment[];
  };
  presentation: {
    sender_display_name?: string | null;
    avatar_url?: string | null;
    group_position: RoomTimelineGroupPosition;
    reply_preview?: BackendRoomTimelineReplyPreview | null;
    permalink?: string | null;
    capabilities: BackendRoomTimelineItemCapabilities;
    compact_receipts: BackendRoomTimelineReceipt[];
    thumbnail?: BackendRoomTimelineThumbnailState | null;
  };
};

export type BackendTimelineRichTextNode =
  | { type: "text"; text: string }
  | {
      type: "element";
      tag: string;
      attributes: BackendTimelineRichTextAttributes;
      children: BackendTimelineRichTextNode[];
    };

export type BackendTimelineRichTextAttributes = {
  href?: string | null;
  target?: string | null;
  alt?: string | null;
  title?: string | null;
  language?: string | null;
  start?: number | null;
  color?: string | null;
  background_color?: string | null;
  spoiler?: string | null;
  maths?: string | null;
};

export type RoomTimelineGroupPosition =
  "standalone" | "start" | "middle" | "end";

type BackendRoomTimelineReaction = {
  key: string;
  count: number;
  reacted_by_me: boolean;
};

type BackendRoomTimelineReceipt = {
  user_id: string;
  display_name?: string | null;
  avatar_url?: string | null;
  timestamp_unix_ms?: number | null;
};

type BackendRoomTimelineThreadRelation = {
  root_event_id: string;
  latest_event_id?: string | null;
  reply_count: number;
};

type BackendRoomTimelineThreadReplyRelation = {
  root_event_id: string;
};

type BackendRoomTimelineAttachment = {
  event_id: string;
  media_type: "image" | "video" | "audio" | "file" | "sticker" | "unknown";
  filename?: string | null;
  mime_type?: string | null;
  width?: number | null;
  height?: number | null;
  size_bytes?: number | null;
};

export type BackendRoomTimelineReplyPreview = {
  event_id: string;
  state?: BackendRoomTimelineReplyPreviewState;
  sender_id?: string | null;
  sender_display_name?: string | null;
  body?: string | null;
  is_redacted: boolean;
};

type BackendRoomTimelineReplyPreviewState =
  | "resolved"
  | "loading"
  | "deleted_redacted"
  | "inaccessible"
  | "failed_to_load"
  | "invalid_relation";

export type RoomTimelineReplyPreviewState =
  | "resolved"
  | "loading"
  | "deletedRedacted"
  | "inaccessible"
  | "failedToLoad"
  | "invalidRelation";

type BackendRoomTimelineItemCapabilities = {
  can_edit: boolean;
  can_redact: boolean;
  can_reply: boolean;
  can_react: boolean;
};

type BackendRoomTimelineThumbnailState = {
  cache_key: string;
  width?: number | null;
  height?: number | null;
  blurhash?: string | null;
};

export type BackendRoomTimeline = {
  room_id: string;
  items: BackendRoomTimelineItem[];
  next_before?: string | null;
  focused_event_id?: string | null;
  redacted_event_ids?: string[];
};

export type BackendRoomTimelinePaginationResponse = {
  room_id: string;
  items: BackendRoomTimelineItem[];
  next_before?: string | null;
  request_id: string;
  had_new_items: boolean;
  returned_item_count: number;
  new_item_count: number;
  duplicate_item_count: number;
  continuation_attempt_count: number;
  token_changed: boolean;
  reason?: string | null;
};

export type RoomTimelineItem = {
  id: string;
  transactionId: string | null;
  senderId: string;
  roomId: string | null;
  senderDisplayName: string;
  senderAvatarUrl: string;
  body: string;
  formattedBody: string;
  formattedBodyFormat: string | null;
  richText: BackendTimelineRichTextNode[] | null;
  contentKind:
    | "text"
    | "pendingDecryption"
    | "unableToDecrypt"
    | "redacted"
    | "nonText"
    | "unsupported";
  timestampUnixMs: number;
  timeLabel: string;
  isEdited: boolean;
  isRedacted: boolean;
  isOwnMessage: boolean;
  sendState: "pending" | "sending" | "sent" | "failed" | "retrying";
  decryptionState: "unencrypted" | "decrypted" | "unableToDecrypt" | "pending";
  groupPosition: RoomTimelineGroupPosition;
  permalink: string;
  canEdit: boolean;
  canRedact: boolean;
  canReply: boolean;
  canReact: boolean;
  reactions: RoomTimelineReaction[];
  receipts: RoomTimelineReceipt[];
  thread: RoomTimelineThreadRelation | null;
  threadReplyTo: RoomTimelineThreadReplyRelation | null;
  replyPreview: RoomTimelineReplyPreview | null;
};

export type RoomTimelineThreadRelation = {
  rootEventId: string;
  latestEventId: string | null;
  replyCount: number;
};

export type RoomTimelineThreadReplyRelation = {
  rootEventId: string;
};

export type RoomTimelineReaction = {
  key: string;
  count: number;
  reactedByMe: boolean;
};

export type RoomTimelineReceipt = {
  userId: string;
  displayName: string;
  avatarUrl: string;
  timestampUnixMs: number | null;
};

export type RoomTimelineReplyPreview = {
  eventId: string;
  state: RoomTimelineReplyPreviewState;
  senderId: string;
  senderDisplayName: string;
  body: string;
  isRedacted: boolean;
};

export type RoomTimeline = {
  roomId: string;
  items: RoomTimelineItem[];
  nextBefore: string | null;
  focusedEventId: string | null;
  redactedEventIds: string[];
};

export type RoomTimelinePaginationResponse = {
  roomId: string;
  items: RoomTimelineItem[];
  nextBefore: string | null;
  requestId: string;
  hadNewItems: boolean;
  returnedItemCount: number;
  newItemCount: number;
  duplicateCount: number;
  continuationAttemptCount: number;
  tokenChanged: boolean;
  reason: string;
};

export type BackendSpaceSummary = {
  space_id: string;
  name: string;
  description: string;
  member_label: string;
  activity_label: string;
  accent_label?: string | null;
  is_official?: boolean;
};

export type SpaceSummary = {
  id: string;
  name: string;
  description: string;
  memberLabel: string;
  activityLabel: string;
  accentLabel: string;
  isOfficial?: boolean;
};

export function mapRoomSummary(
  backendSummary: BackendRoomSummary,
): RoomSummary {
  return {
    id: backendSummary.room_id,
    title: backendSummary.title,
    participantLabel: backendSummary.participant_label,
    homeserverLabel: backendSummary.homeserver_label,
    topic: backendSummary.topic ?? "",
    isDirect: backendSummary.is_direct,
    canSendMessages: backendSummary.can_send_messages,
  };
}

export function mapRoomTimeline(
  backendTimeline: BackendRoomTimeline,
): RoomTimeline {
  return {
    roomId: backendTimeline.room_id,
    items: (backendTimeline.items ?? []).map(mapRoomTimelineItem),
    nextBefore: backendTimeline.next_before ?? null,
    focusedEventId: backendTimeline.focused_event_id ?? null,
    redactedEventIds: backendTimeline.redacted_event_ids ?? [],
  };
}

export function mapRoomTimelinePaginationResponse(
  response: BackendRoomTimelinePaginationResponse,
): RoomTimelinePaginationResponse {
  return {
    roomId: response.room_id,
    items: (response.items ?? []).map(mapRoomTimelineItem),
    nextBefore: response.next_before ?? null,
    requestId: response.request_id,
    hadNewItems: response.had_new_items,
    returnedItemCount: response.returned_item_count,
    newItemCount: response.new_item_count,
    duplicateCount: response.duplicate_item_count,
    continuationAttemptCount: response.continuation_attempt_count,
    tokenChanged: response.token_changed,
    reason: response.reason ?? "",
  };
}

function mapRoomTimelineItem(item: BackendRoomTimelineItem): RoomTimelineItem {
  const capabilities = item.presentation.capabilities ?? {
    can_edit: false,
    can_redact: false,
    can_reply: false,
    can_react: false,
  };

  return {
    id: item.matrix.event_id,
    transactionId: item.matrix.transaction_id ?? null,
    senderId: item.matrix.sender_id,
    roomId: item.matrix.room_id ?? null,
    senderDisplayName:
      item.presentation.sender_display_name?.trim() || item.matrix.sender_id,
    senderAvatarUrl: item.presentation.avatar_url ?? "",
    body: item.matrix.content.body ?? "",
    formattedBody: item.matrix.content.formatted_body ?? "",
    formattedBodyFormat: item.matrix.content.formatted_body_format ?? null,
    richText: item.matrix.content.rich_text ?? null,
    contentKind: mapTimelineContentKind(item.matrix.content.kind),
    timestampUnixMs: item.matrix.timestamp_unix_ms,
    timeLabel: formatTimelineTime(item.matrix.timestamp_unix_ms),
    isEdited: item.matrix.content.is_edited,
    isRedacted: item.matrix.content.is_redacted,
    isOwnMessage: item.matrix.is_own_message,
    sendState: item.matrix.send_state,
    decryptionState: mapTimelineDecryptionState(item.matrix.decryption_state),
    groupPosition: item.presentation.group_position,
    permalink: item.presentation.permalink ?? "",
    canEdit: capabilities.can_edit,
    canRedact: capabilities.can_redact,
    canReply: capabilities.can_reply,
    canReact: capabilities.can_react,
    reactions: (item.matrix.reactions ?? []).map(mapTimelineReaction),
    receipts: (item.presentation.compact_receipts ?? []).map(
      mapTimelineReceipt,
    ),
    thread: item.matrix.thread
      ? {
          rootEventId: item.matrix.thread.root_event_id,
          latestEventId: item.matrix.thread.latest_event_id ?? null,
          replyCount: item.matrix.thread.reply_count,
        }
      : null,
    threadReplyTo: item.matrix.thread_reply_to
      ? { rootEventId: item.matrix.thread_reply_to.root_event_id }
      : null,
    replyPreview: item.presentation.reply_preview
      ? mapTimelineReplyPreview(item.presentation.reply_preview)
      : null,
  };
}

export function mapTimelineReplyPreview(
  replyPreview: BackendRoomTimelineReplyPreview,
): RoomTimelineReplyPreview {
  const senderId = replyPreview.sender_id ?? "";
  return {
    eventId: replyPreview.event_id,
    state: mapReplyPreviewState(replyPreview.state),
    senderId,
    senderDisplayName:
      replyPreview.sender_display_name?.trim() || senderId || "Reply",
    body: replyPreview.body ?? "",
    isRedacted: replyPreview.is_redacted,
  };
}

function mapReplyPreviewState(
  state: BackendRoomTimelineReplyPreview["state"],
): RoomTimelineReplyPreviewState {
  if (state === "deleted_redacted") {
    return "deletedRedacted";
  }

  if (state === "failed_to_load") {
    return "failedToLoad";
  }

  if (state === "invalid_relation") {
    return "invalidRelation";
  }

  if (state === "resolved" || state === "loading" || state === "inaccessible") {
    return state;
  }

  return "loading";
}

function mapTimelineReaction(
  reaction: BackendRoomTimelineReaction,
): RoomTimelineReaction {
  return {
    key: reaction.key,
    count: reaction.count,
    reactedByMe: reaction.reacted_by_me,
  };
}

function mapTimelineReceipt(
  receipt: BackendRoomTimelineReceipt,
): RoomTimelineReceipt {
  return {
    userId: receipt.user_id,
    displayName: receipt.display_name?.trim() || receipt.user_id,
    avatarUrl: receipt.avatar_url ?? "",
    timestampUnixMs: receipt.timestamp_unix_ms ?? null,
  };
}

function mapTimelineContentKind(
  kind: BackendRoomTimelineItem["matrix"]["content"]["kind"],
): RoomTimelineItem["contentKind"] {
  if (kind === "unable_to_decrypt") {
    return "unableToDecrypt";
  }

  if (kind === "pending_decryption") {
    return "pendingDecryption";
  }

  if (kind === "non_text") {
    return "nonText";
  }

  return kind;
}

function mapTimelineDecryptionState(
  state: BackendRoomTimelineItem["matrix"]["decryption_state"],
): RoomTimelineItem["decryptionState"] {
  if (state === "unable_to_decrypt") {
    return "unableToDecrypt";
  }

  return state;
}

export function mapSpaceSummary(
  backendSpace: BackendSpaceSummary,
): SpaceSummary {
  return {
    id: backendSpace.space_id,
    name: backendSpace.name,
    description: backendSpace.description,
    memberLabel: backendSpace.member_label,
    activityLabel: backendSpace.activity_label,
    accentLabel:
      backendSpace.accent_label?.trim() ||
      backendSpace.name.slice(0, 1).toUpperCase() ||
      "S",
    isOfficial: backendSpace.is_official,
  };
}

export function accountInitials(account: AccountSummary): string {
  const sanitizedUserId = account.user_id.replace(/^@/, "");
  const [leadingSegment = "H"] = sanitizedUserId.split(":");
  return leadingSegment.slice(0, 2).toUpperCase();
}

function formatTimelineTime(timestampUnixMs: number): string {
  if (timestampUnixMs <= 0) {
    return "";
  }

  const date = new Date(timestampUnixMs);
  return date.toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
  });
}
