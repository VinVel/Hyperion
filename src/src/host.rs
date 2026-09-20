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

//! Placeholder host boundary for the Qt migration.

use std::{fs, io, path::PathBuf};

use reqwest::Url;

/// Native application capabilities will be supplied by the Qt host.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppHandle;

impl AppHandle {
    pub fn path(&self) -> AppPaths {
        unimplemented!("Qt application paths must be provided by the native host")
    }

    pub fn fs(&self) -> AppFs {
        unimplemented!("Qt application filesystem function must be provided by the native host")
    }

    pub fn dialog(&self) -> Dialog {
        unimplemented!("Qt dialog service must be provided by the native host")
    }

    pub fn emit<S: AsRef<str>, T>(&self, _event: S, _payload: T) -> Result<(), String> {
        unimplemented!("Qt event bridge must be implemented")
    }
}

pub struct AppPaths;
pub struct AppFs;

/// Host-independent representation of a local path or document-provider URL.
#[derive(Clone)]
pub enum FilePath {
    Path(PathBuf),
    Url(Url),
}

/// File-open flags passed from the backend to the native host.
#[derive(Default)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    truncate: bool,
}

impl OpenOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn read(&mut self, enabled: bool) -> &mut Self {
        self.read = enabled;
        self
    }

    pub fn write(&mut self, enabled: bool) -> &mut Self {
        self.write = enabled;
        self
    }

    pub fn create(&mut self, enabled: bool) -> &mut Self {
        self.create = enabled;
        self
    }

    pub fn truncate(&mut self, enabled: bool) -> &mut Self {
        self.truncate = enabled;
        self
    }
}

pub struct Dialog;
pub struct FileDialog;

impl Dialog {
    pub fn file(&self) -> FileDialog {
        unimplemented!("Qt file dialog must be provided by the native host")
    }
}

impl FileDialog {
    pub fn add_filter(self, _name: &str, _extensions: &[&str]) -> Self {
        self
    }

    pub fn set_file_name(self, _file_name: &str) -> Self {
        self
    }

    pub fn blocking_save_file(self) -> Option<FilePath> {
        unimplemented!("Qt save-file dialog must be implemented")
    }

    pub fn blocking_pick_file(self) -> Option<FilePath> {
        unimplemented!("Qt open-file dialog must be implemented")
    }
}

impl AppPaths {
    pub fn app_data_dir(&self) -> Result<std::path::PathBuf, std::io::Error> {
        unimplemented!("Qt application data directory must be provided by the native host")
    }

    pub fn app_cache_dir(&self) -> Result<std::path::PathBuf, std::io::Error> {
        unimplemented!("Qt application cache directory must be provided by the native host")
    }
}

impl AppFs {
    pub fn open(&self, _path: FilePath, _options: OpenOptions) -> Result<fs::File, io::Error> {
        unimplemented!("Qt application open directive must be provided by the native host")
    }
}

impl std::fmt::Display for FilePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Path(path) => path.display().fmt(formatter),
            Self::Url(url) => url.fmt(formatter),
        }
    }
}

/// Replacement for host-managed state injection during the Qt migration.
pub struct State<'a, T: ?Sized>(pub &'a T);

impl<T: ?Sized> std::ops::Deref for State<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.0
    }
}
