use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
use wry::{
    http::{status::StatusCode, Request, Response},
    webview::RequestAsyncResponder,
    Result,
};

use crate::desktop_context::EditQueue;

static MINIFIED: &str = include_str!("./minified.js");

fn module_loader(root_name: &str) -> String {
    format!(
        r#"
<script type="module">
    {MINIFIED}

    function wait_for_request() {{
        fetch(new Request("dioxus://index.html/edits"))
            .then(response => {{
                response.arrayBuffer()
                    .then(bytes => {{
                        run_from_bytes(bytes);
                        wait_for_request();
                    }});
            }})
    }}

    // Wait for the page to load
    window.onload = function() {{
        let rootname = "{root_name}";
        let root_element = window.document.getElementById(rootname);
        if (root_element != null) {{
            initialize(root_element);
            window.ipc.postMessage(serializeIpcMessage("initialize"));
        }}
        wait_for_request();
    }}
</script>
"#
    )
}

pub(super) fn desktop_handler(
    request: &Request<Vec<u8>>,
    responder: RequestAsyncResponder,
    custom_head: Option<String>,
    custom_index: Option<String>,
    root_name: &str,
    edit_queue: &EditQueue,
) {
    // If the request is for the root, we'll serve the index.html file.
    if request.uri().path() == "/" {
        // If a custom index is provided, just defer to that, expecting the user to know what they're doing.
        // we'll look for the closing </body> tag and insert our little module loader there.
        let body = match custom_index {
            Some(custom_index) => custom_index
                .replace("</body>", &format!("{}</body>", module_loader(root_name)))
                .into_bytes(),

            None => {
                // Otherwise, we'll serve the default index.html and apply a custom head if that's specified.
                let mut template = include_str!("./index.html").to_string();

                if let Some(custom_head) = custom_head {
                    template = template.replace("<!-- CUSTOM HEAD -->", &custom_head);
                }

                template
                    .replace("<!-- MODULE LOADER -->", &module_loader(root_name))
                    .into_bytes()
            }
        };

        match Response::builder()
            .header("Content-Type", "text/html")
            .header("Access-Control-Allow-Origin", "*")
            .body(Cow::from(body))
        {
            Ok(response) => {
                responder.respond(response);
                return;
            }
            Err(err) => tracing::error!("error building response: {}", err),
        }
    } else if request.uri().path().trim_matches('/') == "edits" {
        edit_queue.handle_request(responder);
        return;
    }

    // Else, try to serve a file from the filesystem.
    let decoded = urlencoding::decode(request.uri().path().trim_start_matches('/'))
        .expect("expected URL to be UTF-8 encoded");
    let path = PathBuf::from(&*decoded);

    // If the path is relative, we'll try to serve it from the assets directory.
    let mut asset = get_asset_root()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join(&path);

    if !asset.exists() {
        asset = PathBuf::from("/").join(path);
    }

    if asset.exists() {
        let content_type = match get_mime_from_path(&asset) {
            Ok(content_type) => content_type,
            Err(err) => {
                tracing::error!("error getting mime type: {}", err);
                return;
            }
        };
        let asset = match std::fs::read(asset) {
            Ok(asset) => asset,
            Err(err) => {
                tracing::error!("error reading asset: {}", err);
                return;
            }
        };
        match Response::builder()
            .header("Content-Type", content_type)
            .body(Cow::from(asset))
        {
            Ok(response) => {
                responder.respond(response);
                return;
            }
            Err(err) => tracing::error!("error building response: {}", err),
        }
    }

    match Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Cow::from(String::from("Not Found").into_bytes()))
    {
        Ok(response) => {
            responder.respond(response);
            return;
        }
        Err(err) => tracing::error!("error building response: {}", err),
    }
}

#[allow(unreachable_code)]
fn get_asset_root() -> Option<PathBuf> {
    /*
    We're matching exactly how cargo-bundle works.

    - [x] macOS
    - [ ] Windows
    - [ ] Linux (rpm)
    - [ ] Linux (deb)
    - [ ] iOS
    - [ ] Android

    */

    if std::env::var_os("CARGO").is_some() {
        return None;
    }

    // TODO: support for other platforms
    #[cfg(target_os = "macos")]
    {
        let bundle = core_foundation::bundle::CFBundle::main_bundle();
        let bundle_path = bundle.path()?;
        let resources_path = bundle.resources_path()?;
        let absolute_resources_root = bundle_path.join(resources_path);
        let canonical_resources_root = dunce::canonicalize(absolute_resources_root).ok()?;

        return Some(canonical_resources_root);
    }

    None
}

/// Get the mime type from a path-like string
fn get_mime_from_path(trimmed: &Path) -> Result<&'static str> {
    if trimmed.ends_with(".svg") {
        return Ok("image/svg+xml");
    }

    let res = match infer::get_from_path(trimmed)?.map(|f| f.mime_type()) {
        Some(f) => {
            if f == "text/plain" {
                get_mime_by_ext(trimmed)
            } else {
                f
            }
        }
        None => get_mime_by_ext(trimmed),
    };

    Ok(res)
}

/// Get the mime type from a URI using its extension
fn get_mime_by_ext(trimmed: &Path) -> &'static str {
    match trimmed.extension().and_then(|e| e.to_str()) {
        Some("bin") => "application/octet-stream",
        Some("css") => "text/css",
        Some("csv") => "text/csv",
        Some("html") => "text/html",
        Some("ico") => "image/vnd.microsoft.icon",
        Some("js") => "text/javascript",
        Some("json") => "application/json",
        Some("jsonld") => "application/ld+json",
        Some("mjs") => "text/javascript",
        Some("rtf") => "application/rtf",
        Some("svg") => "image/svg+xml",
        Some("mp4") => "video/mp4",
        // Assume HTML when a TLD is found for eg. `dioxus:://dioxuslabs.app` | `dioxus://hello.com`
        Some(_) => "text/html",
        // https://developer.mozilla.org/en-US/docs/Web/HTTP/Basics_of_HTTP/MIME_types/Common_types
        // using octet stream according to this:
        None => "application/octet-stream",
    }
}
