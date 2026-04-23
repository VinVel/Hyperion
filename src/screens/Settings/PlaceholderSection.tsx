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

import { Settings2 } from 'lucide-react';
import { Card, Typography } from '../../components/ui';
import { settingsSections, type SettingsSectionId } from './settingsSections';

type PlaceholderSectionProps = {
  sectionId: SettingsSectionId;
};

export default function PlaceholderSection({ sectionId }: PlaceholderSectionProps) {
  const section = settingsSections.find((item) => item.id === sectionId);

  return (
    <div className="settings-view-section-body">
      <Card className="settings-view-card settings-view-card--placeholder">
        <div className="settings-view-card-head">
          <Settings2 aria-hidden="true" />
          <div className="settings-view-card-copy">
            <Typography variant="h3">{section?.label}</Typography>
            <Typography muted variant="bodySmall">
              Not yet done.
            </Typography>
          </div>
        </div>
      </Card>
    </div>
  );
}
