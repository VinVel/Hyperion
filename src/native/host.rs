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

use std::{fs, io, path::PathBuf, sync::OnceLock};

use reqwest::Url;

#[cfg(target_os = "android")]
#[path = "android.rs"]
mod android;

/// Native application capabilities will be supplied by the Qt host.
#[derive(Clone, Copy, Debug, Default)]
pub struct AppHandle;

impl AppHandle {
    pub fn path(&self) -> AppPaths {
        AppPaths
    }

    pub fn fs(&self) -> AppFs {
        AppFs
    }

    pub fn emit<S: AsRef<str>, T>(&self, _event: S, _payload: T) -> Result<(), String> {
        unimplemented!("Qt event bridge must be implemented")
    }
}

pub struct AppPaths;
pub struct AppFs;

#[derive(Clone)]
struct AppDirectories {
    data: PathBuf,
    cache: PathBuf,
}

static APP_DIRECTORIES: OnceLock<AppDirectories> = OnceLock::new();

/// Store the paths resolved by Qt's StandardPaths QML type during startup.
pub fn set_qt_app_directories(data_url: &str, cache_url: &str) -> Result<(), String> {
    let data = path_from_qt_url(data_url, "application data")?;
    let cache = path_from_qt_url(cache_url, "application cache")?;

    APP_DIRECTORIES
        .set(AppDirectories { data, cache })
        .map_err(|_| "Qt application directories were already initialized".to_owned())
}

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

impl AppPaths {
    pub fn app_data_dir(&self) -> Result<std::path::PathBuf, std::io::Error> {
        app_directories().map(|directories| directories.data.clone())
    }

    pub fn app_cache_dir(&self) -> Result<std::path::PathBuf, std::io::Error> {
        app_directories().map(|directories| directories.cache.clone())
    }
}

impl AppFs {
    pub fn open(&self, path: FilePath, options: OpenOptions) -> Result<fs::File, io::Error> {
        options.validate()?;

        match path {
            FilePath::Path(path) => options.open_local(path),
            FilePath::Url(url) if url.scheme() == "file" => {
                let path = url.to_file_path().map_err(|()| {
                    io::Error::new(io::ErrorKind::InvalidInput, "file URL is not a valid path")
                })?;
                options.open_local(path)
            }
            FilePath::Url(url) => open_document_url(&url, options),
        }
    }
}

impl OpenOptions {
    fn validate(&self) -> Result<(), io::Error> {
        if !self.read && !self.write {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "file must be opened for reading or writing",
            ));
        }
        if (self.create || self.truncate) && !self.write {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "create and truncate require write access",
            ));
        }

        Ok(())
    }

    fn open_local(&self, path: PathBuf) -> Result<fs::File, io::Error> {
        let mut options = fs::OpenOptions::new();
        options
            .read(self.read)
            .write(self.write)
            .create(self.create)
            .truncate(self.truncate);
        options.open(path)
    }
}

fn path_from_qt_url(path_url: &str, location_name: &str) -> Result<PathBuf, String> {
    let url = Url::parse(path_url)
        .map_err(|error| format!("Qt returned an invalid {location_name} location URL: {error}"))?;
    if url.scheme() != "file" {
        return Err(format!(
            "Qt returned a non-local {location_name} location URL"
        ));
    }

    url.to_file_path()
        .map_err(|()| format!("Qt returned an invalid {location_name} location URL"))
}

fn app_directories() -> Result<&'static AppDirectories, io::Error> {
    APP_DIRECTORIES.get().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Qt application directories have not been initialized",
        )
    })
}

#[cfg(target_os = "android")]
fn open_document_url(url: &Url, options: OpenOptions) -> Result<fs::File, io::Error> {
    if url.scheme() != "content" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unsupported Android document URL scheme: {}", url.scheme()),
        ));
    }

    android::open_document_url(url, options)
}

#[cfg(not(target_os = "android"))]
fn open_document_url(url: &Url, _options: OpenOptions) -> Result<fs::File, io::Error> {
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("unsupported document URL scheme: {}", url.scheme()),
    ))
}

#[cfg(target_os = "android")]
impl OpenOptions {
    fn android_provider_mode(&self) -> i32 {
        match (self.read, self.write, self.truncate) {
            (true, false, _) => 0,
            (false, true, false) => 1,
            (false, true, true) => 2,
            (true, true, false) => 1,
            (true, true, true) => 3,
            (false, false, _) => unreachable!("open options are validated before mode selection"),
        }
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

#[cfg(test)]
mod tests {
    #[cfg(not(target_os = "android"))]
    use super::path_from_qt;
    use super::{AppFs, FilePath, OpenOptions};
    use std::{
        fs,
        io::{Read, Write},
        path::PathBuf,
    };

    fn temporary_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "hyperion-host-{name}-{}.tmp",
            rand::random::<u64>()
        ))
    }

    #[test]
    fn opens_local_path_and_file_url_with_standard_file_semantics() {
        let file_path = temporary_path("open");
        let file_url = reqwest::Url::from_file_path(&file_path).expect("path should form URL");
        let mut create_options = OpenOptions::new();
        create_options.write(true).create(true).truncate(true);
        let mut file = AppFs
            .open(FilePath::Url(file_url), create_options)
            .expect("file URL should be writable");
        file.write_all(b"room keys")
            .expect("file contents should be written");
        drop(file);

        let mut read_options = OpenOptions::new();
        read_options.read(true);
        let mut file = AppFs
            .open(FilePath::Path(file_path.clone()), read_options)
            .expect("local path should be readable");
        let mut contents = String::new();
        file.read_to_string(&mut contents)
            .expect("file contents should be read");
        assert_eq!(contents, "room keys");

        fs::remove_file(file_path).expect("temporary file should be removed");
    }

    #[test]
    fn rejects_options_without_access_or_write_for_creation() {
        let path = temporary_path("invalid-options");
        let error = AppFs
            .open(FilePath::Path(path.clone()), OpenOptions::new())
            .expect_err("access-less options should be rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

        let mut create_without_write = OpenOptions::new();
        create_without_write.read(true).create(true);
        let error = AppFs
            .open(FilePath::Path(path), create_without_write)
            .expect_err("create without write access should be rejected");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    #[cfg(not(target_os = "android"))]
    fn reports_unavailable_qt_standard_path() {
        let error = path_from_qt(String::new(), "application data")
            .expect_err("empty Qt paths should not become current-directory paths");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
}
