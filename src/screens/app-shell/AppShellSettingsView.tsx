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

import { Cog } from 'lucide-react';
import { Button, EmptyState } from '../../components/ui';

type AppShellSettingsViewProps = {
  onBackToMessages: () => void;
};

export default function AppShellSettingsView({
  onBackToMessages,
}: AppShellSettingsViewProps) {
  return (
    <section className="app-shell-main-pane app-shell-main-pane--full" aria-label="Settings">
      <EmptyState
        actions={
          <Button onClick={onBackToMessages} variant="primary">
            Back to messages
          </Button>
        }
        copy="Settings are not available yet."
        graphic={<Cog aria-hidden="true" />}
        title="Settings"
      />
    </section>
  );
}
