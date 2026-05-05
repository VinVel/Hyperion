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

import type { BackendGlobalSearchResponse, SearchResultGroup } from './types';

export function globalSearchStatusNotice(
  response: BackendGlobalSearchResponse,
): string | null {
  const status = response.status;
  if (!status || status.state === 'idle') {
    return null;
  }

  if (status.notice?.trim()) {
    return status.notice;
  }

  if (status.state === 'indexing') {
    return 'Older history is still being indexed.';
  }

  if (status.state === 'degraded' || status.state === 'error') {
    return 'Search results may be incomplete while the index is rebuilt.';
  }

  return null;
}

export function mapGlobalSearchResponse(
  response: BackendGlobalSearchResponse,
): SearchResultGroup[] {
  return [
    {
      kind: 'room' as const,
      title: 'Rooms',
      items: response.rooms.map((item) => ({
        id: item.room_id,
        title: item.title,
        description: item.description,
        targetView: 'messages' as const,
        threadId: item.room_id,
      })),
    },
    {
      kind: 'space' as const,
      title: 'Spaces',
      items: response.spaces.map((item) => ({
        id: item.space_id,
        title: item.title,
        description: item.description,
        targetView: 'spaces' as const,
      })),
    },
    {
      kind: 'message' as const,
      title: 'Messages',
      items: response.messages.map((item) => ({
        id: item.result_id,
        title: item.title,
        description: item.description,
        targetView: 'messages' as const,
        threadId: item.room_id,
        eventId: item.event_id ?? undefined,
      })),
    },
  ].filter((group) => group.items.length > 0);
}
