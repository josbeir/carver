use super::*;
use std::{
    cell::RefCell,
    io::{BufRead, Read},
    process::{Command, Stdio},
    rc::Rc,
};

struct PrivateBus(std::process::Child);

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub(crate) fn preview_service_should_receive_a_copy_and_support_portal_export()
-> Result<(), Box<dyn std::error::Error>> {
    let mut daemon = PrivateBus(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let output = daemon.0.stdout.take().ok_or("bus output")?;
    let mut address = String::new();
    std::io::BufReader::new(output).read_line(&mut address)?;
    let connection = gio::DBusConnection::for_address_sync(
        address.trim(),
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )?;
    for name in [SUSHI_BUS, DOCUMENTS] {
        connection.call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(name, 0_u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            3000,
            gio::Cancellable::NONE,
        )?;
    }
    assert_sushi_service(&connection, true)?;
    assert_sushi_service(&connection, false)?;
    assert_fallback_selection()?;
    assert_portal_export(&connection)?;
    connection.close_sync(gio::Cancellable::NONE)?;
    Ok(())
}

fn assert_portal_export(
    connection: &gio::DBusConnection,
) -> Result<(), Box<dyn std::error::Error>> {
    let context = glib::MainContext::default();
    let documents = gio::DBusNodeInfo::for_xml(
        r#"<node><interface name="org.freedesktop.portal.Documents">
        <method name="Add"><arg type="h" direction="in"/><arg type="b" direction="in"/><arg type="b" direction="in"/><arg type="s" direction="out"/></method>
        <method name="GetMountPoint"><arg type="ay" direction="out"/></method>
        </interface></node>"#,
    )?;
    let received = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&received);
    let registration = connection
        .register_object(
            DOCUMENTS_PATH,
            &documents.lookup_interface(DOCUMENTS).ok_or("documents")?,
        )
        .method_call(move |_, _, _, _, method, parameters, invocation| {
            if method == "Add" {
                let Some((handle, reuse, persistent)) =
                    parameters.get::<(glib::variant::Handle, bool, bool)>()
                else {
                    panic!("Add arguments");
                };
                assert!(reuse);
                assert!(!persistent);
                let Some(descriptors) = invocation.message().unix_fd_list() else {
                    panic!("file descriptors");
                };
                let Ok(descriptor) = descriptors.get(handle.0) else {
                    panic!("descriptor");
                };
                let mut file = std::fs::File::from(descriptor);
                assert!(file.read_to_end(&mut captured.borrow_mut()).is_ok());
                invocation.return_value(Some(&("test-copy",).to_variant()));
            } else {
                invocation.return_value(Some(&(b"/tmp/portal\0".to_vec(),).to_variant()));
            }
        })
        .build()?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("Brief.pdf");
    std::fs::write(&path, b"preview bytes")?;
    let file = context.block_on(shared_file_for_viewer(
        &gio::File::for_path(&path),
        Some(connection),
    ))?;
    assert_eq!(received.borrow().as_slice(), b"preview bytes");
    assert_eq!(file.uri(), "file:///tmp/portal/test-copy/Brief.pdf");
    connection.unregister_object(registration)?;
    Ok(())
}

fn assert_sushi_service(
    connection: &gio::DBusConnection,
    with_token: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let xml = format!(
        r#"<node><interface name="org.gnome.NautilusPreviewer2"><method name="ShowFile">
        <arg type="s" direction="in"/><arg type="s" direction="in"/>
        <arg type="b" direction="in"/>{}
        </method></interface></node>"#,
        if with_token {
            r#"<arg type="s" direction="in"/>"#
        } else {
            ""
        },
    );
    let info = gio::DBusNodeInfo::for_xml(&xml)?;
    let calls = Rc::new(RefCell::new(Vec::new()));
    let captured = Rc::clone(&calls);
    let registration = connection
        .register_object(
            SUSHI_PATH,
            &info.lookup_interface(SUSHI_INTERFACE).ok_or("interface")?,
        )
        .method_call(move |_, _, _, _, _, parameters, invocation| {
            captured.borrow_mut().push(parameters);
            invocation.return_value(Some(&().to_variant()));
        })
        .build()?;
    let context = glib::MainContext::default();
    context.block_on(request_sushi(connection, "file:///tmp/Brief.pdf"))?;
    assert_eq!(
        calls.borrow().as_slice(),
        &[if with_token {
            ("file:///tmp/Brief.pdf", "", false, "").to_variant()
        } else {
            ("file:///tmp/Brief.pdf", "", false).to_variant()
        }]
    );
    connection.unregister_object(registration)?;
    assert!(
        context
            .block_on(request_sushi(connection, "file:///tmp/missing.pdf"))
            .is_err(),
        "unavailable preview service must allow the caller to fall back"
    );

    Ok(())
}

fn assert_fallback_selection() -> Result<(), glib::Error> {
    let context = glib::MainContext::default();
    let opened = std::cell::Cell::new(false);
    context.block_on(preview_with_fallback(std::future::ready(Ok(())), async {
        opened.set(true);
        Ok(())
    }))?;
    assert!(
        !opened.get(),
        "a successful preview should not also launch an app"
    );
    context.block_on(preview_with_fallback(
        std::future::ready(Err(invalid_response())),
        async {
            opened.set(true);
            Ok(())
        },
    ))?;
    assert!(
        opened.get(),
        "an unavailable previewer should open the default app"
    );
    Ok(())
}
