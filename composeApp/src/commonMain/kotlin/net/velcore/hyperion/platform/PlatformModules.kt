/*
 * Copyright (c) 2026 VinVel
 * SPDX-License-Identifier: AGPL-3.0-or-later
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

package net.velcore.hyperion.platform

import de.connect2x.trixnity.client.media.MediaStore
import de.connect2x.trixnity.core.model.UserId
import org.koin.core.module.Module

expect suspend fun createRepositoriesModule(): Module

expect suspend fun createMediaStore(userId: UserId): MediaStore

expect suspend fun createCryptoDriverModule(): Module