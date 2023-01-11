#![allow(dead_code)]

use dioxus_core::Template;

use interprocess::local_socket::LocalSocketStream;
use std::io::{BufRead, BufReader};
use tokio::sync::mpsc::UnboundedSender;

pub(crate) fn init(proxy: UnboundedSender<Template<'static>>) {
    std::thread::spawn(move || {
        let temp_file = std::env::temp_dir().join("@dioxusin");
        if let Ok(socket) = LocalSocketStream::connect(temp_file.as_path()) {
            let mut buf_reader = BufReader::new(socket);
            loop {
                let mut buf = String::new();
                match buf_reader.read_line(&mut buf) {
                    Ok(_) => {
                        let template: Template<'static> =
                            serde_json::from_str(Box::leak(buf.into_boxed_str())).unwrap();
                        if proxy.send(template).is_err() {
                            return;
                        }
                    }
                    Err(err) => {
                        if err.kind() != std::io::ErrorKind::WouldBlock {
                            break;
                        }
                    }
                }
            }
        }
    });
}
