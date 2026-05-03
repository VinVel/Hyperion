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

import { X } from 'lucide-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { classNames } from './classNames';
import type { FeedbackTone } from './FeedbackMessage';

export type ToastFeedback = {
  tone: FeedbackTone;
  text: string;
};

type ToastNotification = ToastFeedback & {
  id: number;
};

type ToastListener = (notification: ToastNotification) => void;

const toastListeners = new Set<ToastListener>();
let nextToastId = 1;
let lastToastSignature: string | null = null;
let lastToastTimeMilliseconds = 0;

// Toasts need enough reading time for native errors while still clearing without user action.
const toastDismissDelayMilliseconds = 5200;
// React StrictMode can replay effects in development; this prevents duplicate visual toasts.
const duplicateToastSuppressionWindowMilliseconds = 750;
export const toastVisibilityChangedEvent = 'hyperion://toast-visibility-changed';

export function notifyFeedback(feedback: ToastFeedback) {
  const trimmedText = feedback.text.trim();
  if (trimmedText.length === 0) {
    return;
  }

  const toastSignature = `${feedback.tone}:${trimmedText}`;
  const currentTimeMilliseconds = Date.now();
  const isDuplicateToast =
    toastSignature === lastToastSignature &&
    currentTimeMilliseconds - lastToastTimeMilliseconds <
      duplicateToastSuppressionWindowMilliseconds;
  if (isDuplicateToast) {
    return;
  }

  lastToastSignature = toastSignature;
  lastToastTimeMilliseconds = currentTimeMilliseconds;

  const notification = {
    ...feedback,
    text: trimmedText,
    id: nextToastId,
  };
  nextToastId += 1;

  toastListeners.forEach((listener) => listener(notification));
}

export function useFeedbackToast(feedback: ToastFeedback | null | undefined) {
  const lastToastKeyRef = useRef<string | null>(null);

  useEffect(() => {
    if (!feedback) {
      lastToastKeyRef.current = null;
      return;
    }

    const toastKey = `${feedback.tone}:${feedback.text}`;
    if (toastKey === lastToastKeyRef.current) {
      return;
    }

    lastToastKeyRef.current = toastKey;
    notifyFeedback(feedback);
  }, [feedback]);
}

type ToastProviderProps = {
  children: ReactNode;
};

export function ToastProvider({ children }: ToastProviderProps) {
  const [notifications, setNotifications] = useState<ToastNotification[]>([]);

  useEffect(() => {
    const toastIsVisible = notifications.length > 0;
    document.body.classList.toggle('hyperion-toast-visible', toastIsVisible);
    window.dispatchEvent(new Event(toastVisibilityChangedEvent));

    return () => {
      document.body.classList.remove('hyperion-toast-visible');
      window.dispatchEvent(new Event(toastVisibilityChangedEvent));
    };
  }, [notifications]);

  useEffect(() => {
    function handleNotification(notification: ToastNotification) {
      setNotifications([notification]);
    }

    toastListeners.add(handleNotification);
    return () => {
      toastListeners.delete(handleNotification);
    };
  }, []);

  const closeToast = useCallback((id: number) => {
    setNotifications((currentNotifications) =>
      currentNotifications.filter((notification) => notification.id !== id),
    );
  }, []);

  return (
    <>
      {children}
      <div className="ui-toast-region" aria-live="polite" aria-relevant="additions text">
        {notifications.map((notification) => (
          <ToastItem
            key={notification.id}
            notification={notification}
            onClose={closeToast}
          />
        ))}
      </div>
    </>
  );
}

type ToastItemProps = {
  notification: ToastNotification;
  onClose: (id: number) => void;
};

function ToastItem({ notification, onClose }: ToastItemProps) {
  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      onClose(notification.id);
    }, toastDismissDelayMilliseconds);

    return () => {
      window.clearTimeout(timeoutId);
    };
  }, [notification.id, onClose]);

  return (
    <div
      className={classNames(
        'ui-feedback',
        'ui-toast',
        `ui-feedback--${notification.tone}`,
      )}
      role={notification.tone === 'error' ? 'alert' : 'status'}
    >
      <span className="ui-toast__text">{notification.text}</span>
      <button
        className="ui-toast__close"
        type="button"
        aria-label="Dismiss notification"
        onClick={() => onClose(notification.id)}
      >
        <X aria-hidden="true" />
      </button>
    </div>
  );
}
