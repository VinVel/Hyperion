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

import { Share2 } from "lucide-react";
import { Button, Typography } from "../../../../components/ui";
import type {
  RoomTimelineAttachment,
  RoomTimelineItem,
} from "../../appShellAdapters";
import { copyMediaLink } from "./actions";
import { formatBytes, formatDimensions, formatDuration } from "./presentation";

type MediaDetailsProps = {
  attachment: RoomTimelineAttachment;
  item: RoomTimelineItem;
  onClose: () => void;
};

export default function MediaDetails({
  attachment,
  item,
  onClose,
}: MediaDetailsProps) {
  const rows = [
    attachment.mimeType || attachment.mediaType,
    formatBytes(attachment.sizeBytes),
    formatDimensions(attachment),
    formatDuration(attachment.durationUnixMs),
    item.senderDisplayName,
    item.timeLabel,
  ].filter(Boolean);

  return (
    <div
      className="timeline-media-details"
      role="dialog"
      aria-label="Media details"
    >
      <Typography variant="h3">Media details</Typography>
      {rows.map((row) => (
        <Typography key={row} variant="bodySmall" muted>
          {row}
        </Typography>
      ))}
      <div className="timeline-media-action-row">
        <Button
          variant="ghost"
          onClick={() => void copyMediaLink(item.permalink)}
        >
          <Share2 aria-hidden="true" />
          Copy link
        </Button>
        <Button variant="secondary" onClick={onClose}>
          Close
        </Button>
      </div>
    </div>
  );
}
