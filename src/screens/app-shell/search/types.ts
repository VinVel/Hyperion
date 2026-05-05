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

import type { AuthenticatedShellView } from '../appShellAdapters';

export type BackendGlobalSearchResponse = {
  rooms: Array<{
    room_id: string;
    title: string;
    description: string;
  }>;
  spaces: Array<{
    space_id: string;
    title: string;
    description: string;
  }>;
  messages: Array<{
    result_id: string;
    room_id: string;
    title: string;
    description: string;
    event_id?: string | null;
  }>;
  status?: {
    state: 'idle' | 'indexing' | 'paused' | 'degraded' | 'error';
    indexed_room_count: number;
    total_room_count: number;
    message_count: number;
    last_indexed_at_unix_ms?: number | null;
    notice?: string | null;
  };
};

export type SearchResultKind = 'room' | 'space' | 'message';

export type SearchResultGroup = {
  kind: SearchResultKind;
  title: string;
  items: Array<{
    id: string;
    title: string;
    description: string;
    targetView: AuthenticatedShellView;
    threadId?: string;
    eventId?: string;
  }>;
};
