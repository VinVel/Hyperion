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

import {
  memo,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
} from "react";
import { createPortal } from "react-dom";
import { Download, EyeOff, Info, Play, Share2, ZoomIn } from "lucide-react";
import { Button, Typography } from "../../../../components/ui";
import type {
  RoomTimelineAttachment,
  RoomTimelineItem,
} from "../../appShellAdapters";
import {
  cachedPreparedRoomMedia,
  copyMediaLink,
  saveRoomMedia,
} from "./actions";
import { blurhashDataUrl } from "./blurhash";
import MediaDetails from "./MediaDetails";
import MediaViewer from "./MediaViewer";
import {
  formatBytes,
  mediaAspectRatio,
  mediaReservedWidth,
} from "./presentation";
import { inlineMediaHandle } from "./selection";
import type { PreparedMedia, TimelineMediaItem } from "./types";
import "./TimelineMedia.css";

type TimelineMediaProps = {
  cacheScope: string;
  getGalleryItems: () => TimelineMediaItem[];
  item: RoomTimelineItem;
};

type PreparedMediaByHandle = Record<string, PreparedMedia | "error">;

function TimelineMedia({
  cacheScope,
  getGalleryItems,
  item,
}: TimelineMediaProps) {
  const attachments = item.attachments ?? [];
  const [preparedMediaByHandle, setPreparedMediaByHandle] =
    useState<PreparedMediaByHandle>({});
  const requestedMediaHandlesRef = useRef<Set<string>>(new Set());
  const [revealedHandles, setRevealedHandles] = useState<Set<string>>(
    () => new Set(),
  );
  const [viewerHandle, setViewerHandle] = useState<string | null>(null);
  const [detailsAttachment, setDetailsAttachment] =
    useState<RoomTimelineAttachment | null>(null);

  useEffect(() => {
    requestedMediaHandlesRef.current.clear();
    setPreparedMediaByHandle({});
  }, [cacheScope]);

  useEffect(() => {
    for (const attachment of attachments) {
      const mediaHandle = inlineMediaHandle(attachment);
      if (!mediaHandle) {
        continue;
      }

      if (requestedMediaHandlesRef.current.has(mediaHandle)) {
        continue;
      }

      requestedMediaHandlesRef.current.add(mediaHandle);
      void cachedPreparedRoomMedia(cacheScope, mediaHandle)
        .then((preparedMedia) => {
          setPreparedMediaByHandle((currentMedia) => ({
            ...currentMedia,
            [mediaHandle]: preparedMedia,
          }));
        })
        .catch(() => {
          setPreparedMediaByHandle((currentMedia) => ({
            ...currentMedia,
            [mediaHandle]: "error",
          }));
        });
    }
  }, [attachments, cacheScope]);

  function revealMedia(mediaHandle: string) {
    setRevealedHandles((currentHandles) => {
      const nextHandles = new Set(currentHandles);
      nextHandles.add(mediaHandle);
      return nextHandles;
    });
  }

  function preparedForAttachment(attachment: RoomTimelineAttachment) {
    const mediaHandle = inlineMediaHandle(attachment);
    return mediaHandle ? preparedMediaByHandle[mediaHandle] : undefined;
  }

  if (attachments.length === 0) {
    return null;
  }

  return (
    <div className="timeline-media-stack">
      {attachments.map((attachment) => {
        const preparedMedia = preparedForAttachment(attachment);
        if (attachment.mediaType === "audio") {
          return (
            <AudioMedia
              key={attachment.mediaHandle}
              attachment={attachment}
              item={item}
              preparedMedia={preparedMedia}
              onDetails={setDetailsAttachment}
            />
          );
        }

        if (
          attachment.mediaType === "file" ||
          attachment.mediaType === "unknown"
        ) {
          return (
            <FileMedia
              key={attachment.mediaHandle}
              attachment={attachment}
              item={item}
              onDetails={setDetailsAttachment}
            />
          );
        }

        return (
          <VisualMedia
            key={attachment.mediaHandle}
            attachment={attachment}
            cacheScope={cacheScope}
            getGalleryItems={getGalleryItems}
            item={item}
            preparedMedia={preparedMedia}
            revealed={revealedHandles.has(attachment.mediaHandle)}
            viewerHandle={viewerHandle}
            onCloseViewer={() => setViewerHandle(null)}
            onDetails={setDetailsAttachment}
            onOpenViewer={setViewerHandle}
            onReveal={revealMedia}
          />
        );
      })}

      {detailsAttachment
        ? createPortal(
            <MediaDetails
              attachment={detailsAttachment}
              item={item}
              onClose={() => setDetailsAttachment(null)}
            />,
            document.body,
          )
        : null}
    </div>
  );
}

type VisualMediaProps = {
  attachment: RoomTimelineAttachment;
  cacheScope: string;
  getGalleryItems: () => TimelineMediaItem[];
  item: RoomTimelineItem;
  preparedMedia: PreparedMedia | "error" | undefined;
  revealed: boolean;
  viewerHandle: string | null;
  onCloseViewer: () => void;
  onDetails: (attachment: RoomTimelineAttachment) => void;
  onOpenViewer: (mediaHandle: string) => void;
  onReveal: (mediaHandle: string) => void;
};

function VisualMedia({
  attachment,
  cacheScope,
  getGalleryItems,
  item,
  preparedMedia,
  revealed,
  viewerHandle,
  onCloseViewer,
  onDetails,
  onOpenViewer,
  onReveal,
}: VisualMediaProps) {
  const mediaUrl =
    preparedMedia && preparedMedia !== "error" ? preparedMedia.media_url : "";
  const blurhashUrl = useMemo(
    () => (attachment.blurhash ? blurhashDataUrl(attachment.blurhash) : ""),
    [attachment.blurhash],
  );
  const isHidden = attachment.requiresReveal && !revealed;
  const stageStyle = {
    "--timeline-media-aspect-ratio": mediaAspectRatio(attachment),
    "--timeline-media-reserved-width": mediaReservedWidth(attachment),
    backgroundImage:
      blurhashUrl && !isHidden ? `url("${blurhashUrl}")` : undefined,
  } as CSSProperties;
  const frameClassName = `timeline-media-frame timeline-media-frame--${attachment.mediaType}`;

  return (
    <>
      <div className={frameClassName}>
        <div
          className={`timeline-media-stage${
            isHidden ? " timeline-media-stage--hidden" : ""
          }`}
          style={stageStyle}
        >
          {preparedMedia === "error" ? (
            <div className="timeline-media-error">
              Media could not be loaded
            </div>
          ) : mediaUrl ? (
            <img
              alt={attachment.displayCaption || visualMediaLabel(attachment)}
              decoding="async"
              src={mediaUrl}
            />
          ) : (
            <div
              aria-label="Loading media"
              className="timeline-media-placeholder"
            />
          )}

          {attachment.mediaType === "video" ? (
            <span className="timeline-media-play-indicator" aria-hidden="true">
              <Play />
            </span>
          ) : null}

          {isHidden ? (
            <Button
              aria-label="Reveal media"
              iconOnly
              variant="secondary"
              onClick={() => onReveal(attachment.mediaHandle)}
            >
              <EyeOff aria-hidden="true" />
            </Button>
          ) : null}

          <div className="timeline-media-controls">
            <Button
              aria-label="Open media"
              iconOnly
              variant="ghost"
              onClick={() => onOpenViewer(attachment.mediaHandle)}
            >
              {attachment.mediaType === "video" ? (
                <Play aria-hidden="true" />
              ) : (
                <ZoomIn aria-hidden="true" />
              )}
            </Button>
            <Button
              aria-label="Copy media link"
              iconOnly
              variant="ghost"
              onClick={() => void copyMediaLink(item.permalink)}
            >
              <Share2 aria-hidden="true" />
            </Button>
            <Button
              aria-label="Download media"
              iconOnly
              variant="ghost"
              onClick={() => void saveRoomMedia(attachment.mediaHandle)}
            >
              <Download aria-hidden="true" />
            </Button>
            <Button
              aria-label="Media details"
              iconOnly
              variant="ghost"
              onClick={() => onDetails(attachment)}
            >
              <Info aria-hidden="true" />
            </Button>
          </div>
        </div>
        {attachment.displayCaption ? (
          <div className="timeline-media-caption">
            {attachment.displayCaption}
          </div>
        ) : null}
      </div>

      {viewerHandle === attachment.mediaHandle
        ? createPortal(
            <MediaViewer
              attachment={attachment}
              cacheScope={cacheScope}
              getGalleryItems={getGalleryItems}
              item={item}
              onClose={onCloseViewer}
            />,
            document.body,
          )
        : null}
    </>
  );
}

function visualMediaLabel(attachment: RoomTimelineAttachment): string {
  return attachment.mediaType === "video" ? "Shared video" : "Shared media";
}

type AudioMediaProps = {
  attachment: RoomTimelineAttachment;
  item: RoomTimelineItem;
  preparedMedia: PreparedMedia | "error" | undefined;
  onDetails: (attachment: RoomTimelineAttachment) => void;
};

function AudioMedia({
  attachment,
  item,
  preparedMedia,
  onDetails,
}: AudioMediaProps) {
  const mediaUrl =
    preparedMedia && preparedMedia !== "error" ? preparedMedia.media_url : "";
  return (
    <div className="timeline-media-audio">
      {preparedMedia === "error" ? (
        <div className="timeline-media-error">Audio could not be loaded</div>
      ) : mediaUrl ? (
        <audio controls src={mediaUrl} />
      ) : (
        <div className="timeline-media-placeholder">Loading audio...</div>
      )}
      {attachment.displayCaption ? (
        <Typography variant="bodySmall">{attachment.displayCaption}</Typography>
      ) : null}
      <MediaActionRow
        attachment={attachment}
        item={item}
        onDetails={onDetails}
      />
    </div>
  );
}

type FileMediaProps = {
  attachment: RoomTimelineAttachment;
  item: RoomTimelineItem;
  onDetails: (attachment: RoomTimelineAttachment) => void;
};

function FileMedia({ attachment, item, onDetails }: FileMediaProps) {
  return (
    <div className="timeline-media-file">
      <div className="timeline-media-file-head">
        <span className="timeline-media-file-name">
          {attachment.filename || "Shared file"}
        </span>
        <span className="timeline-media-file-meta">
          {[attachment.mimeType || "File", formatBytes(attachment.sizeBytes)]
            .filter(Boolean)
            .join(" · ")}
        </span>
      </div>
      {attachment.displayCaption ? (
        <Typography variant="bodySmall">{attachment.displayCaption}</Typography>
      ) : null}
      <MediaActionRow
        attachment={attachment}
        item={item}
        onDetails={onDetails}
      />
    </div>
  );
}

type MediaActionRowProps = {
  attachment: RoomTimelineAttachment;
  item: RoomTimelineItem;
  onDetails: (attachment: RoomTimelineAttachment) => void;
};

function MediaActionRow({ attachment, item, onDetails }: MediaActionRowProps) {
  return (
    <div className="timeline-media-action-row">
      <Button
        variant="secondary"
        onClick={() => void saveRoomMedia(attachment.mediaHandle)}
      >
        <Download aria-hidden="true" />
        Download
      </Button>
      <Button
        variant="ghost"
        onClick={() => void copyMediaLink(item.permalink)}
      >
        <Share2 aria-hidden="true" />
        Copy link
      </Button>
      <Button variant="ghost" onClick={() => onDetails(attachment)}>
        <Info aria-hidden="true" />
        Info
      </Button>
    </div>
  );
}

export default memo(TimelineMedia);
