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

import { useMemo, useState } from "react";
import {
  Download,
  Share2,
  SquareArrowLeft,
  SquareArrowRight,
  X,
} from "lucide-react";
import { Button } from "../../../../components/ui";
import type {
  RoomTimelineAttachment,
  RoomTimelineItem,
} from "../../appShellAdapters";
import {
  cachedPreparedRoomMedia,
  copyMediaLink,
  saveRoomMedia,
} from "./actions";
import type { TimelineMediaItem } from "./types";

type MediaViewerProps = {
  attachment: RoomTimelineAttachment;
  cacheScope: string;
  getGalleryItems: () => TimelineMediaItem[];
  item: RoomTimelineItem;
  mediaUrl: string;
  onClose: () => void;
};

export default function MediaViewer({
  attachment,
  cacheScope,
  getGalleryItems,
  item,
  mediaUrl,
  onClose,
}: MediaViewerProps) {
  const [currentMediaItem, setCurrentMediaItem] = useState({
    attachment,
    item,
  });
  const [currentUrl, setCurrentUrl] = useState(mediaUrl);
  const galleryItems = useMemo(getGalleryItems, [getGalleryItems]);
  const currentIndex = useMemo(
    () =>
      galleryItems.findIndex(
        (galleryItem) =>
          galleryItem.attachment.mediaHandle ===
          currentMediaItem.attachment.mediaHandle,
      ),
    [currentMediaItem.attachment.mediaHandle, galleryItems],
  );
  const previousItem = currentIndex > 0 ? galleryItems[currentIndex - 1] : null;
  const nextItem =
    currentIndex >= 0 && currentIndex + 1 < galleryItems.length
      ? galleryItems[currentIndex + 1]
      : null;

  function navigate(nextMediaItem: TimelineMediaItem | null) {
    if (!nextMediaItem) {
      return;
    }

    setCurrentMediaItem(nextMediaItem);
    void cachedPreparedRoomMedia(
      cacheScope,
      nextMediaItem.attachment.mediaHandle,
    ).then((preparedMedia) => {
      setCurrentUrl(preparedMedia.media_url);
    });
  }

  return (
    <div className="timeline-media-viewer" role="dialog" aria-modal="true">
      <div className="timeline-media-viewer-bar">
        <Button
          aria-label="Copy media link"
          iconOnly
          variant="ghost"
          onClick={() => void copyMediaLink(currentMediaItem.item.permalink)}
        >
          <Share2 aria-hidden="true" />
        </Button>
        <Button
          aria-label="Download media"
          iconOnly
          variant="ghost"
          onClick={() =>
            void saveRoomMedia(currentMediaItem.attachment.mediaHandle)
          }
        >
          <Download aria-hidden="true" />
        </Button>
        <Button
          aria-label="Close viewer"
          iconOnly
          variant="ghost"
          onClick={onClose}
        >
          <X aria-hidden="true" />
        </Button>
      </div>
      <div className="timeline-media-viewer-stage">
        <Button
          aria-label="Previous media"
          disabled={!previousItem}
          iconOnly
          variant="ghost"
          onClick={() => navigate(previousItem)}
        >
          <SquareArrowLeft aria-hidden="true" />
        </Button>
        <img
          alt={currentMediaItem.attachment.displayCaption || "Shared media"}
          className="timeline-media-viewer-image"
          decoding="async"
          src={currentUrl}
        />
        <Button
          aria-label="Next media"
          disabled={!nextItem}
          iconOnly
          variant="ghost"
          onClick={() => navigate(nextItem)}
        >
          <SquareArrowRight aria-hidden="true" />
        </Button>
      </div>
    </div>
  );
}
