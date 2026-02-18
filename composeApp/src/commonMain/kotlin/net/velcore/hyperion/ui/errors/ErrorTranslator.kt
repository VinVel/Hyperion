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

package net.velcore.hyperion.ui.errors

import androidx.compose.runtime.Composable
import de.connect2x.trixnity.core.ErrorResponse
import hyperion.composeapp.generated.resources.Res
import hyperion.composeapp.generated.resources.attr_device_id
import hyperion.composeapp.generated.resources.attr_identifier
import hyperion.composeapp.generated.resources.attr_identifier_user
import hyperion.composeapp.generated.resources.attr_initial_device_display_name
import hyperion.composeapp.generated.resources.attr_password
import hyperion.composeapp.generated.resources.attr_type
import hyperion.composeapp.generated.resources.error_M_APPSERVICE_LOGIN_UNSUPPORTED
import hyperion.composeapp.generated.resources.error_M_BAD_JSON
import hyperion.composeapp.generated.resources.error_M_BAD_STATE
import hyperion.composeapp.generated.resources.error_M_CANNOT_LEAVE_SERVER_NOTICE_ROOM
import hyperion.composeapp.generated.resources.error_M_CAPTCHA_INVALID
import hyperion.composeapp.generated.resources.error_M_CAPTCHA_NEEDED
import hyperion.composeapp.generated.resources.error_M_DUPLICATE_ANNOTATION
import hyperion.composeapp.generated.resources.error_M_EXCLUSIVE
import hyperion.composeapp.generated.resources.error_M_FORBIDDEN
import hyperion.composeapp.generated.resources.error_M_GUEST_ACCESS_FORBIDDEN
import hyperion.composeapp.generated.resources.error_M_INCOMPATIBLE_ROOM_VERSION
import hyperion.composeapp.generated.resources.error_M_INVALID_PARAM
import hyperion.composeapp.generated.resources.error_M_INVALID_ROOM_STATE
import hyperion.composeapp.generated.resources.error_M_INVALID_USERNAME
import hyperion.composeapp.generated.resources.error_M_LIMIT_EXCEEDED
import hyperion.composeapp.generated.resources.error_M_MISSING_PARAM
import hyperion.composeapp.generated.resources.error_M_MISSING_TOKEN
import hyperion.composeapp.generated.resources.error_M_NOT_FOUND
import hyperion.composeapp.generated.resources.error_M_NOT_JSON
import hyperion.composeapp.generated.resources.error_M_RESOURCE_LIMIT_EXCEEDED
import hyperion.composeapp.generated.resources.error_M_ROOM_IN_USE
import hyperion.composeapp.generated.resources.error_M_SERVER_NOT_TRUSTED
import hyperion.composeapp.generated.resources.error_M_THREEPID_AUTH_FAILED
import hyperion.composeapp.generated.resources.error_M_THREEPID_DENIED
import hyperion.composeapp.generated.resources.error_M_THREEPID_IN_USE
import hyperion.composeapp.generated.resources.error_M_THREEPID_MEDIUM_NOT_SUPPORTED
import hyperion.composeapp.generated.resources.error_M_THREEPID_NOT_FOUND
import hyperion.composeapp.generated.resources.error_M_TOO_LARGE
import hyperion.composeapp.generated.resources.error_M_UNABLE_TO_AUTHORISE_JOIN
import hyperion.composeapp.generated.resources.error_M_UNABLE_TO_GRANT_JOIN
import hyperion.composeapp.generated.resources.error_M_UNAUTHORIZED
import hyperion.composeapp.generated.resources.error_M_UNKNOWN
import hyperion.composeapp.generated.resources.error_M_UNKNOWN_TOKEN
import hyperion.composeapp.generated.resources.error_M_UNRECOGNIZED
import hyperion.composeapp.generated.resources.error_M_UNSUPPORTED_ROOM_VERSION
import hyperion.composeapp.generated.resources.error_M_USER_DEACTIVATED
import hyperion.composeapp.generated.resources.error_M_USER_IN_USE
import hyperion.composeapp.generated.resources.error_M_USER_SUSPENDED
import hyperion.composeapp.generated.resources.error_M_WRONG_ROOM_KEYS_VERSION
import hyperion.composeapp.generated.resources.error_unknown
import hyperion.composeapp.generated.resources.validation_email
import hyperion.composeapp.generated.resources.validation_length_eq
import hyperion.composeapp.generated.resources.validation_length_max
import hyperion.composeapp.generated.resources.validation_length_min
import hyperion.composeapp.generated.resources.validation_range
import hyperion.composeapp.generated.resources.validation_required
import hyperion.composeapp.generated.resources.validation_url
import net.velcore.hyperion.errors.ErrorItem
import org.jetbrains.compose.resources.stringResource

object ErrorTranslator {

    @Composable
    fun toStrings(error: ErrorItem): List<String> {

        val baseMessage = translateBase(error.baseError)

        val validationMessages =
            error.details?.flatMap { (field, rules) ->
                rules.map { rule ->
                    translateValidation(field, rule)
                }
            } ?: emptyList()

        return if (validationMessages.isNotEmpty()) {
            validationMessages + baseMessage
        } else {
            listOf(baseMessage)
        }
    }

    @Composable
    private fun translateBase(error: ErrorResponse): String {
        return when (error) {
            //AUTH & ACCESS
            is ErrorResponse.Forbidden -> stringResource(Res.string.error_M_FORBIDDEN)
            is ErrorResponse.Unauthorized -> stringResource(Res.string.error_M_UNAUTHORIZED)
            is ErrorResponse.UnknownToken ->  stringResource(Res.string.error_M_UNKNOWN_TOKEN)
            is ErrorResponse.MissingToken -> stringResource(Res.string.error_M_MISSING_TOKEN)
            is ErrorResponse.UserDeactivated -> stringResource(Res.string.error_M_USER_DEACTIVATED)
            is ErrorResponse.UserSuspended -> stringResource(Res.string.error_M_USER_SUSPENDED)
            is ErrorResponse.AppserviceLoginUnsupported -> stringResource(Res.string.error_M_APPSERVICE_LOGIN_UNSUPPORTED)

            //VALIDATION & REQUEST
            is ErrorResponse.BadJson -> stringResource(Res.string.error_M_BAD_JSON)
            is ErrorResponse.NotJson -> stringResource(Res.string.error_M_NOT_JSON)
            is ErrorResponse.MissingParam -> stringResource(Res.string.error_M_MISSING_PARAM)
            is ErrorResponse.InvalidParam -> stringResource(Res.string.error_M_INVALID_PARAM)
            is ErrorResponse.TooLarge -> stringResource(Res.string.error_M_TOO_LARGE)
            is ErrorResponse.Unrecognized -> stringResource(Res.string.error_M_UNRECOGNIZED)

            // USER / REGISTRATION
            is ErrorResponse.UserInUse -> stringResource(Res.string.error_M_USER_IN_USE)
            is ErrorResponse.InvalidUsername -> stringResource(Res.string.error_M_INVALID_USERNAME)

            //ROOM
            is ErrorResponse.RoomInUse -> stringResource(Res.string.error_M_ROOM_IN_USE)
            is ErrorResponse.InvalidRoomState -> stringResource(Res.string.error_M_INVALID_ROOM_STATE)
            is ErrorResponse.BadState -> stringResource(Res.string.error_M_BAD_STATE)
            is ErrorResponse.GuestAccessForbidden -> stringResource(Res.string.error_M_GUEST_ACCESS_FORBIDDEN)
            is ErrorResponse.UnsupportedRoomVersion -> stringResource(Res.string.error_M_UNSUPPORTED_ROOM_VERSION)
            is ErrorResponse.IncompatibleRoomVersion -> stringResource(Res.string.error_M_INCOMPATIBLE_ROOM_VERSION)
            is ErrorResponse.WrongRoomKeysVersion -> stringResource(Res.string.error_M_WRONG_ROOM_KEYS_VERSION)
            is ErrorResponse.UnableToAuthoriseJoin -> stringResource(Res.string.error_M_UNABLE_TO_AUTHORISE_JOIN)
            is ErrorResponse.UnableToGrantJoin -> stringResource(Res.string.error_M_UNABLE_TO_GRANT_JOIN)
            is ErrorResponse.CannotLeaveServerNoticeRoom -> stringResource(Res.string.error_M_CANNOT_LEAVE_SERVER_NOTICE_ROOM)
            is ErrorResponse.DuplicateAnnotation -> stringResource(Res.string.error_M_DUPLICATE_ANNOTATION)

            //THREEPID
            is ErrorResponse.ThirdPIdInUse -> stringResource(Res.string.error_M_THREEPID_IN_USE)
            is ErrorResponse.ThirdPartyMediumNotSupported -> stringResource(Res.string.error_M_THREEPID_MEDIUM_NOT_SUPPORTED)
            is ErrorResponse.ThirdPIdNotFound -> stringResource(Res.string.error_M_THREEPID_NOT_FOUND)
            is ErrorResponse.ThirdPIdAuthFailed -> stringResource(Res.string.error_M_THREEPID_AUTH_FAILED)
            is ErrorResponse.ThirdPIdDenied -> stringResource(Res.string.error_M_THREEPID_DENIED)

            //SERVER
            is ErrorResponse.ServerNotTrusted -> stringResource(Res.string.error_M_SERVER_NOT_TRUSTED)
            is ErrorResponse.NotFound -> stringResource(Res.string.error_M_NOT_FOUND)
            is ErrorResponse.LimitExceeded -> stringResource(Res.string.error_M_LIMIT_EXCEEDED)
            is ErrorResponse.ResourceLimitExceeded -> stringResource(Res.string.error_M_RESOURCE_LIMIT_EXCEEDED)
            is ErrorResponse.Exclusive -> stringResource(Res.string.error_M_EXCLUSIVE)
            is ErrorResponse.Unknown -> stringResource(Res.string.error_M_UNKNOWN)

            //CAPTCHA
            is ErrorResponse.CaptchaNeeded -> stringResource(Res.string.error_M_CAPTCHA_NEEDED)
            is ErrorResponse.CaptchaInvalid -> stringResource(Res.string.error_M_CAPTCHA_INVALID)

            else -> stringResource(Res.string.error_unknown)
        }
    }

    @Composable
    private fun translateValidation(field: String, rule: String): String {

        val attribute = when (field) {
            "type" -> stringResource(Res.string.attr_type)
            "password" -> stringResource(Res.string.attr_password)
            "identifier" -> stringResource(Res.string.attr_identifier)
            "identifier.user" -> stringResource(Res.string.attr_identifier_user)
            "device_id" -> stringResource(Res.string.attr_device_id)
            "initial_device_display_name" -> stringResource(Res.string.attr_initial_device_display_name)

            else -> field
        }

        return when {
            rule.startsWith("h.required") ->
                stringResource(Res.string.validation_required, attribute)

            rule.startsWith("h.length.max") -> {
                val max = rule.substringAfter(":")
                stringResource(Res.string.validation_length_max, attribute, max)
            }

            rule.startsWith("h.length.min") -> {
                val min = rule.substringAfter(":")
                stringResource(Res.string.validation_length_min, attribute, min)
            }

            rule.startsWith("h.length.eq") -> {
                val length = rule.substringAfter(":")
                stringResource(Res.string.validation_length_eq, attribute, length)
            }

            rule.startsWith("h.email") ->
                stringResource(Res.string.validation_email, attribute)

            rule.startsWith("h.url") ->
                stringResource(Res.string.validation_url, attribute)

            rule.startsWith("h.range") -> {
                val parts = rule.substringAfter(":").split(",")
                if (parts.size == 2)
                    stringResource(Res.string.validation_range,attribute,parts[0],parts[1])
                else rule
            }

            else -> rule
        }
    }
}