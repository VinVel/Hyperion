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

package net.velcore.hyperion.androidsecurestorage

import android.app.Activity
import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.nio.charset.StandardCharsets
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

private const val KEYSTORE_NAME = "AndroidKeyStore"
private const val MASTER_KEY_ALIAS = "net.velcore.hyperion.secure-store.master"
private const val PREFS_NAME = "hyperion_secure_store"
private const val GCM_TAG_LENGTH = 128

@InvokeArg
class SecretKeyArgs {
  lateinit var key: String
}

@InvokeArg
class SetSecretArgs {
  lateinit var key: String
  lateinit var valueBase64: String
}

@TauriPlugin
class AndroidSecureStoragePlugin(private val activity: Activity) : Plugin(activity) {
  private val preferences by lazy {
    activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
  }

  @Command
  fun getSecret(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SecretKeyArgs::class.java)
      val storedValue = preferences.getString(args.key, null)

      if (storedValue.isNullOrBlank()) {
        val response = JSObject()
        response.put("valueBase64", null)
        invoke.resolve(response)
        return
      }

      val parts = storedValue.split(":", limit = 2)
      if (parts.size != 2) {
        invoke.reject("Stored secure value is malformed")
        return
      }

      val iv = Base64.decode(parts[0], Base64.NO_WRAP)
      val ciphertext = Base64.decode(parts[1], Base64.NO_WRAP)
      val cipher = Cipher.getInstance("AES/GCM/NoPadding")
      cipher.init(Cipher.DECRYPT_MODE, getOrCreateMasterKey(), GCMParameterSpec(GCM_TAG_LENGTH, iv))

      val decrypted = cipher.doFinal(ciphertext)
      val valueBase64 = String(decrypted, StandardCharsets.UTF_8)
      val response = JSObject()
      response.put("valueBase64", valueBase64)
      invoke.resolve(response)
    } catch (error: Exception) {
      invoke.reject("Failed to read Android secure storage: ${error.message}")
    }
  }

  @Command
  fun setSecret(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SetSecretArgs::class.java)
      val cipher = Cipher.getInstance("AES/GCM/NoPadding")
      cipher.init(Cipher.ENCRYPT_MODE, getOrCreateMasterKey())

      val encrypted = cipher.doFinal(args.valueBase64.toByteArray(StandardCharsets.UTF_8))
      val payload =
        "${Base64.encodeToString(cipher.iv, Base64.NO_WRAP)}:" +
          Base64.encodeToString(encrypted, Base64.NO_WRAP)

      preferences.edit().putString(args.key, payload).apply()
      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("Failed to write Android secure storage: ${error.message}")
    }
  }

  @Command
  fun deleteSecret(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(SecretKeyArgs::class.java)
      preferences.edit().remove(args.key).apply()
      invoke.resolve()
    } catch (error: Exception) {
      invoke.reject("Failed to delete Android secure storage entry: ${error.message}")
    }
  }

  private fun getOrCreateMasterKey(): SecretKey {
    val keyStore = KeyStore.getInstance(KEYSTORE_NAME).apply { load(null) }
    val existingKey = keyStore.getKey(MASTER_KEY_ALIAS, null)
    if (existingKey is SecretKey) {
      return existingKey
    }

    val keyGenerator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE_NAME)
    val keySpec =
      KeyGenParameterSpec.Builder(
          MASTER_KEY_ALIAS,
          KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
        )
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setKeySize(256)
        .build()

    keyGenerator.init(keySpec)
    return keyGenerator.generateKey()
  }
}
