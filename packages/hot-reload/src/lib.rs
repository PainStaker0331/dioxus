use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};

use dioxus_core::Template;
use dioxus_html::HtmlCtx;
use dioxus_rsx::hot_reload::{FileMap, UpdateResult};
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// Initialize the hot reloading listener on the given path
pub fn init(path: &'static str, listening_paths: &'static [&'static str]) {
    if let Ok(crate_dir) = PathBuf::from_str(path) {
        let temp_file = std::env::temp_dir().join("@dioxusin");
        let channels = Arc::new(Mutex::new(Vec::new()));
        let file_map = Arc::new(Mutex::new(FileMap::<HtmlCtx>::new(crate_dir.clone())));
        if let Ok(local_socket_stream) = LocalSocketListener::bind(temp_file.as_path()) {
            // listen for connections
            std::thread::spawn({
                let file_map = file_map.clone();
                let channels = channels.clone();
                move || {
                    for connection in local_socket_stream.incoming() {
                        if let Ok(mut connection) = connection {
                            // send any templates than have changed before the socket connected
                            let templates: Vec<_> = {
                                file_map
                                    .lock()
                                    .unwrap()
                                    .map
                                    .values()
                                    .filter_map(|(_, template_slot)| *template_slot)
                                    .collect()
                            };
                            for template in templates {
                                if !send_template(template, &mut connection) {
                                    continue;
                                }
                            }
                            channels.lock().unwrap().push(connection);
                            println!("Connected to hot reloading 🚀");
                        }
                    }
                }
            });

            // watch for changes
            std::thread::spawn(move || {
                let mut last_update_time = chrono::Local::now().timestamp();

                let (tx, rx) = std::sync::mpsc::channel();

                let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).unwrap();

                let mut examples_path = crate_dir.clone();
                examples_path.push("examples");
                let _ = watcher.watch(&examples_path, RecursiveMode::Recursive);
                let mut src_path = crate_dir.clone();
                src_path.push("src");
                let _ = watcher.watch(&src_path, RecursiveMode::Recursive);

                for evt in rx {
                    // Give time for the change to take effect before reading the file
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    if chrono::Local::now().timestamp() > last_update_time {
                        if let Ok(evt) = evt {
                            let mut channels = channels.lock().unwrap();
                            for path in &evt.paths {
                                // skip non rust files
                                if path.extension().and_then(|p| p.to_str()) != Some("rs") {
                                    continue;
                                }

                                // find changes to the rsx in the file
                                match file_map
                                    .lock()
                                    .unwrap()
                                    .update_rsx(&path, crate_dir.as_path())
                                {
                                    UpdateResult::UpdatedRsx(msgs) => {
                                        for msg in msgs {
                                            let mut i = 0;
                                            while i < channels.len() {
                                                let channel = &mut channels[i];
                                                if send_template(msg, channel) {
                                                    i += 1;
                                                } else {
                                                    channels.remove(i);
                                                }
                                            }
                                        }
                                    }
                                    UpdateResult::NeedsRebuild => {
                                        println!("Rebuild needed... shutting down hot reloading");
                                        return;
                                    }
                                }
                            }
                        }
                        last_update_time = chrono::Local::now().timestamp();
                    }
                }
            });
        }
    }
}

fn send_template(template: Template<'static>, channel: &mut impl Write) -> bool {
    if let Ok(msg) = serde_json::to_string(&template) {
        if channel.write_all(msg.as_bytes()).is_err() {
            return false;
        }
        if channel.write_all(&[b'\n']).is_err() {
            return false;
        }
        true
    } else {
        false
    }
}

/// Connect to the hot reloading listener. The callback provided will be called every time a template change is detected
pub fn connect(mut f: impl FnMut(Template<'static>) + Send + 'static) {
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
                        f(template);
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

#[macro_export]
macro_rules! hot_reload {
    () => {
        dioxus_hot_reload::init(core::env!("CARGO_MANIFEST_DIR"), &[])
    };

    ($($paths: literal,)*,?) => {
        dioxus_hot_reload::init(core::env!("CARGO_MANIFEST_DIR"), &[$($path,)*])
    };
}
