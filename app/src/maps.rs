//! The player's maps (WestPlay's `maps` folder, built by the host: `westplay_host::maps`), served to the
//! companion's own pages at `westplay-maps://` (`http://westplay-maps.localhost/` in WebView2): the
//! onboarding shows the map on its live WestPlay as soon as it's made. And the player's own wallpapers and
//! icon sets (`customize`, `westplay_host::customize`) at `westplay-customize://`, for Customize. Read-only,
//! and nothing outside each folder.

use std::path::{Component, Path, PathBuf};

use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Manager, Runtime, UriSchemeContext, UriSchemeResponder};

pub const SCHEME: &str = "westplay-maps";
pub const CUSTOMIZE_SCHEME: &str = "westplay-customize";

/// Answers a request for a file of the maps folder (its tiles are fetched by MapLibre's worker, from another
/// origin: allowed).
pub fn serve<R: Runtime>(context: UriSchemeContext<'_, R>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    serve_folder(context, request, responder, "maps");
}

/// Answers a request for one of the player's wallpapers or icons.
pub fn serve_customize<R: Runtime>(
    context: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    serve_folder(context, request, responder, westplay_host::customize::FOLDER);
}

fn serve_folder<R: Runtime>(
    context: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
    folder: &str,
) {
    let maps = context.app_handle().path().app_local_data_dir().map(|dir| dir.join(folder));
    let path = request.uri().path().to_owned();
    std::thread::spawn(move || {
        let response = match maps.ok().and_then(|maps| file_in(&maps, &path)) {
            Some(file) => match std::fs::read(&file) {
                Ok(bytes) => respond(StatusCode::OK, content_type(&file), bytes),
                Err(_) => respond(StatusCode::NOT_FOUND, "text/plain", Vec::new()),
            },
            None => respond(StatusCode::NOT_FOUND, "text/plain", Vec::new()),
        };
        responder.respond(response);
    });
}

fn respond(status: StatusCode, content_type: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .unwrap_or_default()
}

/// The file a request's path names in the folder `maps`: only plain names below it (no `..`, no drive or root).
fn file_in(maps: &Path, path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    let relative = Path::new(decoded.trim_start_matches('/'));
    if relative.as_os_str().is_empty() || !relative.components().all(|part| matches!(part, Component::Normal(_))) {
        return None;
    }
    Some(maps.join(relative))
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn content_type(file: &Path) -> &'static str {
    // A player's image keeps its own name: `Dusk.JPG` as much as `dusk.jpg`.
    let extension = file.extension().and_then(|extension| extension.to_str()).map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("json") => "application/json",
        Some("pbf") => "application/x-protobuf",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_only_what_is_in_the_maps_folder() {
        let maps = Path::new(r"C:\data\maps");
        assert_eq!(
            file_in(maps, "/europe-cbf29ce484222325/tiles-1/5/16/10.pbf"),
            Some(maps.join("europe-cbf29ce484222325/tiles-1/5/16/10.pbf"))
        );
        assert_eq!(file_in(maps, "/my%20map/map.json"), Some(maps.join("my map/map.json")));
        assert_eq!(file_in(maps, "/../settings.json"), None);
        assert_eq!(file_in(maps, "/europe/%2E%2E/%2E%2E/secret"), None);
        assert_eq!(file_in(maps, "/C:/Windows/win.ini"), None);
        assert_eq!(file_in(maps, "/"), None);
    }
}
