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

//! iOS document picking that keeps security-scoped URLs inside the UIKit callback.

use std::{cell::RefCell, fs, path::PathBuf};

use dispatch2::DispatchQueue;
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained, runtime::ProtocolObject,
};
use objc2_foundation::{NSArray, NSObject, NSString, NSURL};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerDelegate, UIDocumentPickerMode, UIDocumentPickerViewController,
    UIViewController,
};
use tokio::sync::oneshot;

type PickerResult = Result<Option<()>, String>;
type PickerSender = oneshot::Sender<PickerResult>;

enum PickerRequest {
    Import { destination: PathBuf },
    ExportWithSource(PathBuf),
}

enum PickerAction {
    Import { destination: PathBuf },
    Export,
}

struct PickerIvars {
    action: PickerAction,
    sender: RefCell<Option<PickerSender>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = PickerIvars]
    struct RoomKeyPickerDelegate;

    unsafe impl objc2_foundation::NSObjectProtocol for RoomKeyPickerDelegate {}

    unsafe impl UIDocumentPickerDelegate for RoomKeyPickerDelegate {
        #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
        fn did_pick_documents(
            &self,
            picker: &UIDocumentPickerViewController,
            urls: &NSArray<NSURL>,
        ) {
            let result = match &self.ivars().action {
                PickerAction::Import { destination } => urls
                    .firstObject()
                    .ok_or_else(|| String::from("The iOS picker returned no document"))
                    .and_then(|url| copy_scoped_document(&url, destination)),
                PickerAction::Export => Ok(()),
            }
            .map(Some);

            finish_picker(self, picker, result);
        }

        #[unsafe(method(documentPickerWasCancelled:))]
        fn document_picker_was_cancelled(&self, picker: &UIDocumentPickerViewController) {
            finish_picker(self, picker, Ok(None));
        }
    }
);

thread_local! {
    // UIKit keeps a document picker delegate weakly, so retain the active one here.
    static ACTIVE_PICKER_DELEGATE: RefCell<Option<Retained<RoomKeyPickerDelegate>>> = const { RefCell::new(None) };
}

/// Present iOS Files and copy the selected document into app-owned storage before the
/// security-scoped URL leaves the UIKit callback.
pub async fn pick_import_file(destination: PathBuf) -> PickerResult {
    present_picker(PickerRequest::Import { destination }).await
}

/// Present iOS Files to export a staged room-key file. The picker copies the staged
/// document to the selected provider before the async call completes.
pub async fn pick_export_file(source: PathBuf) -> PickerResult {
    present_picker(PickerRequest::ExportWithSource(source)).await
}

async fn present_picker(request: PickerRequest) -> PickerResult {
    let (sender, receiver) = oneshot::channel();
    DispatchQueue::main().exec_async(move || {
        present_picker_on_main(request, sender);
    });

    receiver
        .await
        .map_err(|_| String::from("The iOS document picker closed unexpectedly"))?
}

fn present_picker_on_main(request: PickerRequest, sender: PickerSender) {
    let Some(main_thread) = MainThreadMarker::new() else {
        let _ = sender.send(Err(String::from(
            "The iOS picker must run on the main thread",
        )));
        return;
    };

    let mut active_picker = None;
    ACTIVE_PICKER_DELEGATE.with(|active| {
        active_picker = active.borrow_mut().take();
    });
    if active_picker.is_some() {
        let _ = sender.send(Err(String::from(
            "Another iOS document picker is already open",
        )));
        ACTIVE_PICKER_DELEGATE.with(|active| *active.borrow_mut() = active_picker);
        return;
    }

    let Some(view_controller) = top_view_controller(main_thread) else {
        let _ = sender.send(Err(String::from("Could not find the active iOS window")));
        return;
    };

    let (delegate_action, picker) = match request {
        PickerRequest::Import { destination } => {
            let content_types = NSArray::from_retained_slice(&[NSString::from_str("public.data")]);
            #[allow(deprecated)]
            let picker = UIDocumentPickerViewController::initWithDocumentTypes_inMode(
                UIDocumentPickerViewController::alloc(main_thread),
                &content_types,
                UIDocumentPickerMode::Open,
            );
            (PickerAction::Import { destination }, picker)
        }
        PickerRequest::ExportWithSource(source) => {
            let url = NSURL::fileURLWithPath(&NSString::from_str(&source.to_string_lossy()));
            let source_urls = NSArray::from_retained_slice(&[url]);
            let picker = UIDocumentPickerViewController::initForExportingURLs_asCopy(
                UIDocumentPickerViewController::alloc(main_thread),
                &source_urls,
                true,
            );
            (PickerAction::Export, picker)
        }
    };

    let delegate = RoomKeyPickerDelegate::alloc(main_thread).set_ivars(PickerIvars {
        action: delegate_action,
        sender: RefCell::new(Some(sender)),
    });
    let delegate: Retained<RoomKeyPickerDelegate> = unsafe {
        // SAFETY: RoomKeyPickerDelegate directly subclasses NSObject and initializes its
        // ivars before invoking NSObject's standard initializer.
        msg_send![super(delegate), init]
    };
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    ACTIVE_PICKER_DELEGATE.with(|active| *active.borrow_mut() = Some(delegate));
    view_controller.presentViewController_animated_completion(&picker, true, None);
}

fn top_view_controller(main_thread: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let application = UIApplication::sharedApplication(main_thread);
    // Qt currently hosts one iOS scene; use its key window to find the presentation root.
    #[allow(
        deprecated,
        reason = "Qt currently creates one scene for this app window"
    )]
    let windows = application.windows();
    let key_window = windows.iter().find(|window| window.isKeyWindow())?;
    let mut current = key_window.rootViewController()?;
    while let Some(presented) = current.presentedViewController() {
        current = presented;
    }
    Some(current)
}

fn copy_scoped_document(url: &NSURL, destination: &PathBuf) -> Result<(), String> {
    let source = url
        .path()
        .ok_or_else(|| String::from("The selected iOS document has no local path"))?
        .to_string();
    // SAFETY: This is the original NSURL supplied by UIDocumentPicker, which carries the
    // security scope granted for this selection. The matching stop call is below.
    let accessed = unsafe { url.startAccessingSecurityScopedResource() };
    if !accessed {
        let source_path = fs::canonicalize(source)
            .map_err(|error| format!("Failed to resolve selected document path: {error}"))?;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .and_then(|home| fs::canonicalize(home).ok());
        if !home.is_some_and(|home| source_path.starts_with(home)) {
            return Err(String::from(
                "iOS could not grant access to the selected document",
            ));
        }
        return fs::copy(source_path, destination)
            .map(|_| ())
            .map_err(|error| format!("Failed to copy the selected room-key file: {error}"));
    }
    let _scope = SecurityScope(url);

    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| format!("Failed to copy the selected room-key file: {error}"))
}

struct SecurityScope<'a>(&'a NSURL);

impl Drop for SecurityScope<'_> {
    fn drop(&mut self) {
        // SAFETY: startAccessingSecurityScopedResource returned true for this URL.
        unsafe { self.0.stopAccessingSecurityScopedResource() };
    }
}

fn finish_picker(
    delegate: &RoomKeyPickerDelegate,
    picker: &UIDocumentPickerViewController,
    result: PickerResult,
) {
    if let Some(sender) = delegate.ivars().sender.borrow_mut().take() {
        let _ = sender.send(result);
    }
    picker.dismissViewControllerAnimated_completion(true, None);
    ACTIVE_PICKER_DELEGATE.with(|active| {
        active.borrow_mut().take();
    });
}
