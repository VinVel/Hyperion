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

use std::{
    fs, io,
    os::fd::{FromRawFd, OwnedFd},
    sync::OnceLock,
};

use jni::{
    Env, JavaVM,
    objects::{Global, JObject, JValue},
};
use reqwest::Url;

use super::OpenOptions;

static JAVA_VM: OnceLock<JavaVM> = OnceLock::new();
static QT_NATIVE_CLASS: OnceLock<Global<jni::objects::JClass<'static>>> = OnceLock::new();

/// Cache Qt's Android Java class when Android loads the native application library.
#[unsafe(export_name = "JNI_OnLoad")]
pub unsafe extern "system" fn jni_on_load(
    raw_vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) -> jni::sys::jint {
    if raw_vm.is_null() {
        return jni::sys::JNI_ERR;
    }

    // SAFETY: Android passes the process JavaVM to JNI_OnLoad and keeps it alive for the process.
    let vm = unsafe { JavaVM::from_raw(raw_vm) };
    let loaded = vm.attach_current_thread(|env| {
        let class = env.find_class(jni::jni_str!("org/qtproject/qt/android/QtNative"))?;
        let global_class = env.new_global_ref(class)?;
        // Transfer the new global reference to the process-lifetime cache below.
        let raw_class = global_class.into_raw();
        // SAFETY: raw_class is the owned global reference created immediately above.
        let global_class =
            unsafe { env.global_from_raw::<jni::objects::JClass<'static>>(raw_class) };
        Ok::<_, jni::errors::Error>(global_class)
    });

    let Ok(global_class) = loaded else {
        return jni::sys::JNI_ERR;
    };
    if JAVA_VM.set(vm).is_err() || QT_NATIVE_CLASS.set(global_class).is_err() {
        return jni::sys::JNI_ERR;
    }

    jni::sys::JNI_VERSION_1_6
}

pub(super) fn open_document_url(url: &Url, options: OpenOptions) -> Result<fs::File, io::Error> {
    let provider_mode = provider_mode(options.android_provider_mode())?;
    let vm = android_vm()?;
    let raw_fd = vm
        .attach_current_thread(|env| {
            let context = android_context(env)?;
            let resolver = env
                .call_method(
                    &context,
                    jni::jni_str!("getContentResolver"),
                    jni::jni_sig!("()Landroid/content/ContentResolver;"),
                    &[],
                )?
                .l()?;
            let uri_string = env.new_string(url.as_str())?;
            let java_uri = env
                .call_static_method(
                    jni::jni_str!("android/net/Uri"),
                    jni::jni_str!("parse"),
                    jni::jni_sig!("(Ljava/lang/String;)Landroid/net/Uri;"),
                    &[JValue::Object(&uri_string)],
                )?
                .l()?;
            let java_mode = env.new_string(provider_mode)?;
            let descriptor = env
                .call_method(
                    &resolver,
                    jni::jni_str!("openFileDescriptor"),
                    jni::jni_sig!(
                        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;"
                    ),
                    &[JValue::Object(&java_uri), JValue::Object(&java_mode)],
                )?
                .l()?;

            if descriptor.is_null() {
                return Err(jni::errors::Error::NullPtr("openFileDescriptor result"));
            }

            env.call_method(
                &descriptor,
                jni::jni_str!("detachFd"),
                jni::jni_sig!("()I"),
                &[],
            )?
            .i()
        })
        .map_err(jni_error)?;

    if raw_fd < 0 {
        return Err(io::Error::other(
            "Android document provider returned an invalid file descriptor",
        ));
    }

    // SAFETY: ParcelFileDescriptor::detachFd transfers the descriptor ownership to Rust.
    let owned_fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    Ok(fs::File::from(owned_fd))
}

fn android_vm() -> Result<JavaVM, io::Error> {
    JAVA_VM
        .get()
        .cloned()
        .ok_or_else(|| io::Error::other("Android Java VM has not been initialized"))
}

fn android_context<'local>(env: &mut Env<'local>) -> jni::errors::Result<Global<JObject<'static>>> {
    let class = QT_NATIVE_CLASS
        .get()
        .ok_or(jni::errors::Error::NullPtr("Qt Android class"))?;
    let context = env
        .call_static_method(
            class,
            jni::jni_str!("getContext"),
            jni::jni_sig!("()Landroid/content/Context;"),
            &[],
        )?
        .l()?;
    if context.is_null() {
        return Err(jni::errors::Error::NullPtr("Qt Android context"));
    }

    env.new_global_ref(context)
}

fn provider_mode(mode: i32) -> Result<&'static str, io::Error> {
    match mode {
        0 => Ok("r"),
        // Android providers lack a write-only, non-truncating mode; rw preserves content.
        1 => Ok("rw"),
        2 => Ok("wt"),
        3 => Ok("rwt"),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid Android document provider mode",
        )),
    }
}

fn jni_error(error: jni::errors::Error) -> io::Error {
    io::Error::other(format!("Android JNI operation failed: {error}"))
}
