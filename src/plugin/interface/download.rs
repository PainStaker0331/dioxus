use std::{io::Cursor, path::PathBuf};

use mlua::UserData;

pub struct PluginDownloader;
impl UserData for PluginDownloader {
    fn add_methods<'lua, M: mlua::UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_async_function("download_file", |_, args: (String, String)| async move {
            let url = args.0;
            let path = args.1;

            let resp = reqwest::get(url).await;
            if let Ok(resp) = resp {
                let mut content = Cursor::new(resp.bytes().await.unwrap());
                let file = std::fs::File::create(PathBuf::from(path));
                if file.is_err() {
                    return Ok(false);
                }
                let mut file = file.unwrap();
                let res = std::io::copy(&mut content, &mut file);
                return Ok(res.is_ok());
            }

            Ok(false)
        });
    }
}
