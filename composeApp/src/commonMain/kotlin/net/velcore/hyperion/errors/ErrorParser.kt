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

package net.velcore.hyperion.errors

import de.connect2x.trixnity.core.ErrorResponse
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

object ErrorParser {

    private val json = Json { ignoreUnknownKeys = true }

    fun parse(errorResponse: ErrorResponse, rawBody: String?): ErrorItem {
        val details = rawBody?.let { extractDetails(it) }
        return ErrorItem(
            baseError = errorResponse,
            details = details
        )
    }

    private fun extractDetails(body: String): Map<String, List<String>>? {
        return try {
            val obj = json.parseToJsonElement(body).jsonObject
            val details = obj["details"]?.jsonObject ?: return null

            details.mapValues { entry ->
                entry.value.jsonArray.map { it.jsonPrimitive.content }
            }
        }
        catch (_: Throwable) {
            null
        }
    }
}