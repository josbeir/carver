//! Optional Sushi integration with a portal-aware default-app fallback.

use gtk::{gio, prelude::*};
use std::{
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

use crate::mvu::{AppDispatcher, AppMsg, EditorMsg, EditorSessionId};

const SUSHI_BUS: &str = "org.gnome.NautilusPreviewer";
const SUSHI_PATH: &str = "/org/gnome/NautilusPreviewer";
const SUSHI_INTERFACE: &str = "org.gnome.NautilusPreviewer2";
const DOCUMENTS: &str = "org.freedesktop.portal.Documents";
const DOCUMENTS_PATH: &str = "/org/freedesktop/portal/documents";

pub(super) fn launch(
    path: &Path,
    parent: Option<&gtk::Window>,
    session: EditorSessionId,
    dispatcher: &AppDispatcher,
) {
    let file = gio::File::for_path(path);
    let parent = parent.cloned();
    let dispatcher = dispatcher.clone();
    glib::spawn_future_local(async move {
        let launcher = gtk::FileLauncher::new(Some(&file));
        launcher.set_writable(false);
        if let Err(error) =
            preview_with_fallback(try_sushi(&file), launcher.launch_future(parent.as_ref())).await
            && !error.matches(gtk::DialogError::Dismissed)
            && !error.matches(gio::IOErrorEnum::Cancelled)
        {
            let _ = dispatcher.dispatch(AppMsg::Editor(EditorMsg::MediaPreviewFailed { session }));
        }
    });
}

async fn preview_with_fallback(
    preview: impl std::future::Future<Output = Result<(), glib::Error>>,
    open: impl std::future::Future<Output = Result<(), glib::Error>>,
) -> Result<(), glib::Error> {
    if preview.await.is_ok() {
        Ok(())
    } else {
        open.await
    }
}

async fn try_sushi(file: &gio::File) -> Result<(), glib::Error> {
    let connection = gio::bus_get_future(gio::BusType::Session).await?;
    // Export only this user-requested copy; host viewers cannot resolve sandbox-private paths.
    let uri =
        if std::path::Path::new("/.flatpak-info").exists() || std::env::var_os("SNAP").is_some() {
            export_document(&connection, file).await?
        } else {
            file.uri().to_string()
        };
    request_sushi(&connection, &uri).await
}

async fn request_sushi(connection: &gio::DBusConnection, uri: &str) -> Result<(), glib::Error> {
    let parameters = (uri, "", false, "").to_variant();
    let result = connection
        .call_future(
            Some(SUSHI_BUS),
            SUSHI_PATH,
            SUSHI_INTERFACE,
            "ShowFile",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await;
    if let Err(error) = result {
        if !error.matches(gio::DBusError::InvalidArgs) {
            return Err(error);
        }
        // Older Sushi releases use the same interface without the activation token.
        connection
            .call_future(
                Some(SUSHI_BUS),
                SUSHI_PATH,
                SUSHI_INTERFACE,
                "ShowFile",
                Some(&(uri, "", false).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                3000,
            )
            .await?;
    }
    Ok(())
}

async fn export_document(
    connection: &gio::DBusConnection,
    file: &gio::File,
) -> Result<String, glib::Error> {
    let path = file.path().ok_or_else(invalid_response)?;
    let stream = gio::spawn_blocking(move || std::fs::File::open(path))
        .await
        .map_err(|_| invalid_response())?
        .map_err(|error| glib::Error::new(gio::IOErrorEnum::Failed, &error.to_string()))?;
    let descriptors = gio::UnixFDList::new();
    let descriptor = descriptors.append(&stream)?;
    let parameters = (glib::variant::Handle(descriptor), true, false).to_variant();
    let (reply, _) = connection
        .call_with_unix_fd_list_future(
            Some(DOCUMENTS),
            DOCUMENTS_PATH,
            DOCUMENTS,
            "Add",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            Some(&descriptors),
        )
        .await?;
    let (id,) = reply.get::<(String,)>().ok_or_else(invalid_response)?;
    let mount = connection
        .call_future(
            Some(DOCUMENTS),
            DOCUMENTS_PATH,
            DOCUMENTS,
            "GetMountPoint",
            None,
            None,
            gio::DBusCallFlags::NONE,
            3000,
        )
        .await?;
    let (mut bytes,) = mount.get::<(Vec<u8>,)>().ok_or_else(invalid_response)?;
    if bytes.last() == Some(&0) {
        bytes.pop();
    }
    let filename = file.basename().ok_or_else(invalid_response)?;
    let path = PathBuf::from(std::ffi::OsString::from_vec(bytes))
        .join(id)
        .join(filename);
    Ok(gio::File::for_path(path).uri().to_string())
}

fn invalid_response() -> glib::Error {
    glib::Error::new(
        gio::IOErrorEnum::InvalidData,
        "Invalid document portal response",
    )
}

#[cfg(test)]
pub(crate) mod tests;
