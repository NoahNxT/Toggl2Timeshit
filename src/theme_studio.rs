use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io;
use std::net::{IpAddr, SocketAddr, TcpListener, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, ResponseBox, Server, StatusCode};

use crate::storage::{self, ThemeConfigError, ThemeDraft, ThemeSettings};
use crate::theme::{
    BuiltinTheme, CustomTheme, ThemePalette, ThemeSelection, builtin_themes, validate_theme_name,
};

const STUDIO_HOST: &str = "timeshit.studio.localhost";
const IDLE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const INDEX_HTML: &str = include_str!("../theme-studio/dist/index.html");
const APP_JS: &str = include_str!("../theme-studio/dist/assets/app.js");
const APP_CSS: &str = include_str!("../theme-studio/dist/assets/app.css");

#[derive(Debug)]
pub enum ThemeStudioError {
    Io(String),
    Bind(String),
    Browser(String),
    Serde(String),
}

impl std::fmt::Display for ThemeStudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "{message}"),
            Self::Bind(message) => write!(f, "{message}"),
            Self::Browser(message) => write!(f, "{message}"),
            Self::Serde(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ThemeStudioError {}

impl From<io::Error> for ThemeStudioError {
    fn from(value: io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<serde_json::Error> for ThemeStudioError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serde(value.to_string())
    }
}

impl From<ThemeConfigError> for ThemeStudioError {
    fn from(value: ThemeConfigError) -> Self {
        match value {
            ThemeConfigError::Io(err) => Self::Io(err.to_string()),
            ThemeConfigError::Validation(message) => Self::Io(message),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeStudioExit {
    Closed,
    TimedOut,
}

pub fn run() -> Result<ThemeStudioExit, ThemeStudioError> {
    run_with_store(Arc::new(FileThemeStore))
}

fn run_with_store(store: Arc<dyn ThemeStore>) -> Result<ThemeStudioExit, ThemeStudioError> {
    let listeners = bind_theme_studio_listeners(STUDIO_HOST)?;
    let port = listeners
        .first()
        .and_then(|listener| listener.local_addr().ok())
        .map(|addr| addr.port())
        .ok_or_else(|| {
            ThemeStudioError::Bind("Could not determine theme studio port.".to_string())
        })?;
    let url = format!("http://{STUDIO_HOST}:{port}/");

    let shared = Arc::new(SharedRuntime {
        last_activity: Mutex::new(Instant::now()),
        stop: AtomicBool::new(false),
        store,
    });
    let (finish_tx, finish_rx) = mpsc::channel();
    let mut handles = spawn_http_servers(listeners, shared.clone(), finish_tx)?;

    if let Err(err) = open_browser(&url) {
        shared.stop.store(true, Ordering::SeqCst);
        join_server_threads(&mut handles);
        return Err(err);
    }

    println!("Theme studio available at {url}");

    let exit = wait_for_exit(shared.clone(), finish_rx);
    shared.stop.store(true, Ordering::SeqCst);
    join_server_threads(&mut handles);
    Ok(exit)
}

trait ThemeStore: Send + Sync {
    fn read(&self) -> Result<ThemeSettings, ThemeStudioError>;
    fn save_custom_theme(
        &self,
        draft: ThemeDraft,
    ) -> Result<(ThemeSettings, String), ThemeStudioError>;
    fn delete_custom_theme(&self, id: &str) -> Result<ThemeSettings, ThemeStudioError>;
    fn activate(&self, selection: &ThemeSelection) -> Result<ThemeSettings, ThemeStudioError>;
}

struct FileThemeStore;

impl ThemeStore for FileThemeStore {
    fn read(&self) -> Result<ThemeSettings, ThemeStudioError> {
        Ok(storage::read_theme_settings())
    }

    fn save_custom_theme(
        &self,
        draft: ThemeDraft,
    ) -> Result<(ThemeSettings, String), ThemeStudioError> {
        let normalized_name = validate_theme_name(&draft.name).map_err(ThemeStudioError::Io)?;
        let settings = storage::save_custom_theme(draft.clone())?;
        let saved_theme_id = if let Some(id) = draft.id {
            id
        } else {
            settings
                .custom_themes
                .iter()
                .find(|theme| theme.name == normalized_name)
                .map(|theme| theme.id.clone())
                .ok_or_else(|| {
                    ThemeStudioError::Io("Saved theme could not be resolved.".to_string())
                })?
        };
        Ok((settings, saved_theme_id))
    }

    fn delete_custom_theme(&self, id: &str) -> Result<ThemeSettings, ThemeStudioError> {
        storage::delete_custom_theme(id).map_err(ThemeStudioError::from)
    }

    fn activate(&self, selection: &ThemeSelection) -> Result<ThemeSettings, ThemeStudioError> {
        storage::write_theme_selection(selection).map_err(ThemeStudioError::from)
    }
}

struct SharedRuntime {
    last_activity: Mutex<Instant>,
    stop: AtomicBool,
    store: Arc<dyn ThemeStore>,
}

fn spawn_http_servers(
    listeners: Vec<TcpListener>,
    shared: Arc<SharedRuntime>,
    finish_tx: mpsc::Sender<()>,
) -> Result<Vec<thread::JoinHandle<()>>, ThemeStudioError> {
    let mut handles = Vec::new();

    for listener in listeners {
        let server = Server::from_listener(listener, None)
            .map_err(|err| ThemeStudioError::Bind(err.to_string()))?;
        let thread_shared = shared.clone();
        let thread_finish_tx = finish_tx.clone();
        handles.push(thread::spawn(move || {
            loop {
                if thread_shared.stop.load(Ordering::SeqCst) {
                    break;
                }

                match server.recv_timeout(POLL_INTERVAL) {
                    Ok(Some(request)) => {
                        if let Ok(mut last_activity) = thread_shared.last_activity.lock() {
                            *last_activity = Instant::now();
                        }
                        let should_finish =
                            handle_http_request(request, thread_shared.store.as_ref());
                        if should_finish {
                            let _ = thread_finish_tx.send(());
                        }
                    }
                    Ok(None) => {}
                    Err(_) => break,
                }
            }
        }));
    }

    Ok(handles)
}

fn join_server_threads(handles: &mut Vec<thread::JoinHandle<()>>) {
    while let Some(handle) = handles.pop() {
        let _ = handle.join();
    }
}

fn wait_for_exit(shared: Arc<SharedRuntime>, finish_rx: mpsc::Receiver<()>) -> ThemeStudioExit {
    loop {
        if finish_rx.recv_timeout(Duration::from_secs(1)).is_ok() {
            return ThemeStudioExit::Closed;
        }

        let timed_out = shared
            .last_activity
            .lock()
            .map(|last| last.elapsed() >= IDLE_TIMEOUT)
            .unwrap_or(false);
        if timed_out {
            return ThemeStudioExit::TimedOut;
        }
    }
}

fn bind_theme_studio_listeners(host: &str) -> Result<Vec<TcpListener>, ThemeStudioError> {
    let addresses = resolve_loopback_addresses(host)?;
    let Some(primary_ip) = addresses.first().copied() else {
        return Err(ThemeStudioError::Bind(
            "Theme studio hostname did not resolve to a loopback address.".to_string(),
        ));
    };

    for _ in 0..32 {
        let primary = TcpListener::bind(SocketAddr::new(primary_ip, 0))
            .map_err(|err| ThemeStudioError::Bind(err.to_string()))?;
        let port = primary
            .local_addr()
            .map_err(|err| ThemeStudioError::Bind(err.to_string()))?
            .port();

        let mut listeners = vec![primary];
        let mut ok = true;

        for ip in addresses.iter().copied().skip(1) {
            match TcpListener::bind(SocketAddr::new(ip, port)) {
                Ok(listener) => listeners.push(listener),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }

        if ok {
            return Ok(listeners);
        }
    }

    Err(ThemeStudioError::Bind(
        "Could not reserve a shared random loopback port for the theme studio.".to_string(),
    ))
}

fn resolve_loopback_addresses(host: &str) -> Result<Vec<IpAddr>, ThemeStudioError> {
    let addresses = format!("{host}:0")
        .to_socket_addrs()
        .map_err(|err| ThemeStudioError::Bind(err.to_string()))?;
    let mut ips = BTreeSet::new();

    for address in addresses {
        if address.ip().is_loopback() {
            ips.insert(address.ip());
        }
    }

    Ok(ips.into_iter().collect())
}

fn open_browser(url: &str) -> Result<(), ThemeStudioError> {
    let mut command = match std::env::consts::OS {
        "macos" => {
            let mut command = std::process::Command::new("open");
            command.arg(url);
            command
        }
        "windows" => {
            let mut command = std::process::Command::new("cmd");
            command.args(["/C", "start", "", url]);
            command
        }
        _ => {
            let mut command = std::process::Command::new("xdg-open");
            command.arg(url);
            command
        }
    };

    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|err| ThemeStudioError::Browser(err.to_string()))?;

    Ok(())
}

fn handle_http_request(mut request: Request, store: &dyn ThemeStore) -> bool {
    let mut body = String::new();
    let _ = request.as_reader().read_to_string(&mut body);
    let path = request.url().to_string();
    let result = route_request(request.method(), &path, &body, store);
    let should_finish = result.should_finish;
    let _ = request.respond(result.response);
    should_finish
}

fn route_request(method: &Method, path: &str, body: &str, store: &dyn ThemeStore) -> HttpReply {
    if path == "/" && matches!(method, Method::Get) {
        return HttpReply::html(INDEX_HTML);
    }
    if path == "/assets/app.js" && matches!(method, Method::Get) {
        return HttpReply::javascript(APP_JS);
    }
    if path == "/assets/app.css" && matches!(method, Method::Get) {
        return HttpReply::css(APP_CSS);
    }
    if path == "/api/state" && matches!(method, Method::Get) {
        return json_reply(match store.read() {
            Ok(settings) => Ok(state_payload(&settings)),
            Err(err) => Err(api_error(500, err.to_string())),
        });
    }
    if path == "/api/themes" && matches!(method, Method::Post) {
        return json_reply(handle_save_theme(body, store));
    }
    if path.starts_with("/api/themes/") && matches!(method, Method::Delete) {
        let id = path.trim_start_matches("/api/themes/");
        return json_reply(match store.delete_custom_theme(id) {
            Ok(settings) => Ok(ActivateResponse {
                state: state_payload(&settings),
            }),
            Err(err) => Err(api_error(400, err.to_string())),
        });
    }
    if path == "/api/activate" && matches!(method, Method::Post) {
        let selection = match serde_json::from_str::<ThemeSelection>(body) {
            Ok(selection) => selection,
            Err(err) => return HttpReply::json_error(400, err.to_string()),
        };
        return json_reply(match store.activate(&selection) {
            Ok(settings) => Ok(ActivateResponse {
                state: state_payload(&settings),
            }),
            Err(err) => Err(api_error(400, err.to_string())),
        });
    }
    if path == "/api/finish" && matches!(method, Method::Post) {
        return HttpReply::no_content(true);
    }

    HttpReply::json_error(404, "Not found".to_string())
}

fn handle_save_theme(body: &str, store: &dyn ThemeStore) -> Result<SaveResponse, ApiError> {
    let payload = serde_json::from_str::<ThemeDraftPayload>(body)
        .map_err(|err| api_error(400, err.to_string()))?;
    let draft = ThemeDraft {
        id: payload.id,
        name: payload.name,
        palette: payload.palette,
    };
    let (settings, saved_theme_id) = store
        .save_custom_theme(draft)
        .map_err(|err| api_error(400, err.to_string()))?;
    Ok(SaveResponse {
        state: state_payload(&settings),
        saved_theme_id,
    })
}

fn state_payload(settings: &ThemeSettings) -> StateResponse {
    StateResponse {
        builtins: builtin_themes(),
        custom_themes: settings.custom_themes.clone(),
        active_theme: settings.active_theme.clone(),
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateResponse {
    builtins: Vec<BuiltinTheme>,
    custom_themes: Vec<CustomTheme>,
    active_theme: ThemeSelection,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveResponse {
    state: StateResponse,
    saved_theme_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivateResponse {
    state: StateResponse,
}

#[derive(Debug, Serialize, Deserialize)]
struct ThemeDraftPayload {
    id: Option<String>,
    name: String,
    palette: ThemePalette,
}

#[derive(Debug)]
struct ApiError {
    status: u16,
    message: String,
}

fn api_error(status: u16, message: String) -> ApiError {
    ApiError { status, message }
}

fn json_reply<T: Serialize>(result: Result<T, ApiError>) -> HttpReply {
    match result {
        Ok(payload) => match serde_json::to_vec(&payload) {
            Ok(body) => HttpReply::json(StatusCode(200), body),
            Err(err) => HttpReply::json_error(500, err.to_string()),
        },
        Err(err) => HttpReply::json_error(err.status, err.message),
    }
}

struct HttpReply {
    response: ResponseBox,
    should_finish: bool,
}

impl HttpReply {
    fn html(body: &str) -> Self {
        Self::text(
            StatusCode(200),
            "text/html; charset=utf-8",
            body.as_bytes().to_vec(),
            false,
        )
    }

    fn javascript(body: &str) -> Self {
        Self::text(
            StatusCode(200),
            "application/javascript; charset=utf-8",
            body.as_bytes().to_vec(),
            false,
        )
    }

    fn css(body: &str) -> Self {
        Self::text(
            StatusCode(200),
            "text/css; charset=utf-8",
            body.as_bytes().to_vec(),
            false,
        )
    }

    fn json(status: StatusCode, body: Vec<u8>) -> Self {
        Self::text(status, "application/json; charset=utf-8", body, false)
    }

    fn json_error(status: u16, message: String) -> Self {
        let body = serde_json::to_vec(&ErrorPayload { error: message })
            .unwrap_or_else(|_| b"{\"error\":\"Unknown error\"}".to_vec());
        Self::text(
            StatusCode(status),
            "application/json; charset=utf-8",
            body,
            false,
        )
    }

    fn no_content(should_finish: bool) -> Self {
        let mut response = Response::empty(StatusCode(204));
        if let Ok(header) = Header::from_bytes(b"Cache-Control".as_slice(), b"no-store".as_slice())
        {
            response.add_header(header);
        }
        Self {
            response: response.boxed(),
            should_finish,
        }
    }

    fn text(status: StatusCode, content_type: &str, body: Vec<u8>, should_finish: bool) -> Self {
        let mut response = Response::from_data(body).with_status_code(status);
        if let Ok(header) = Header::from_bytes(b"Content-Type".as_slice(), content_type.as_bytes())
        {
            response.add_header(header);
        }
        if let Ok(header) = Header::from_bytes(b"Cache-Control".as_slice(), b"no-store".as_slice())
        {
            response.add_header(header);
        }

        Self {
            response: response.boxed(),
            should_finish,
        }
    }
}

#[derive(Serialize)]
struct ErrorPayload {
    error: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemePreference;

    struct MemoryThemeStore {
        settings: Mutex<ThemeSettings>,
        next_id: Mutex<u32>,
    }

    impl MemoryThemeStore {
        fn new() -> Self {
            Self {
                settings: Mutex::new(ThemeSettings {
                    active_theme: ThemeSelection::builtin(ThemePreference::Dark),
                    fallback_theme: ThemePreference::Dark,
                    custom_themes: vec![CustomTheme {
                        id: "theme-1".to_string(),
                        name: "Aurora".to_string(),
                        palette: ThemePalette {
                            panel: "#111111".to_string(),
                            border: "#222222".to_string(),
                            text: "#eeeeee".to_string(),
                            muted: "#999999".to_string(),
                            accent: "#00f5ff".to_string(),
                            highlight: "#ff59c2".to_string(),
                            success: "#6fffb1".to_string(),
                            error: "#ff608d".to_string(),
                        },
                        created_at: "2026-03-27T11:00:00+01:00".to_string(),
                        updated_at: "2026-03-27T11:00:00+01:00".to_string(),
                    }],
                }),
                next_id: Mutex::new(2),
            }
        }
    }

    impl ThemeStore for MemoryThemeStore {
        fn read(&self) -> Result<ThemeSettings, ThemeStudioError> {
            Ok(self.settings.lock().unwrap().clone())
        }

        fn save_custom_theme(
            &self,
            draft: ThemeDraft,
        ) -> Result<(ThemeSettings, String), ThemeStudioError> {
            let mut settings = self.settings.lock().unwrap();
            let name = validate_theme_name(&draft.name).map_err(ThemeStudioError::Io)?;
            let palette = draft.palette.normalized().map_err(ThemeStudioError::Io)?;
            let id = if let Some(id) = draft.id {
                let theme = settings
                    .custom_themes
                    .iter_mut()
                    .find(|theme| theme.id == id)
                    .ok_or_else(|| ThemeStudioError::Io("Theme not found".to_string()))?;
                theme.name = name.clone();
                theme.palette = palette;
                theme.updated_at = "updated".to_string();
                id
            } else {
                let mut counter = self.next_id.lock().unwrap();
                let id = format!("theme-{}", *counter);
                *counter += 1;
                settings.custom_themes.push(CustomTheme {
                    id: id.clone(),
                    name,
                    palette,
                    created_at: "created".to_string(),
                    updated_at: "created".to_string(),
                });
                id
            };
            settings
                .custom_themes
                .sort_by_key(|theme| theme.name.to_ascii_lowercase());
            Ok((settings.clone(), id))
        }

        fn delete_custom_theme(&self, id: &str) -> Result<ThemeSettings, ThemeStudioError> {
            let mut settings = self.settings.lock().unwrap();
            settings.custom_themes.retain(|theme| theme.id != id);
            if matches!(settings.active_theme, ThemeSelection::Custom { id: ref active_id } if active_id == id)
            {
                settings.active_theme = ThemeSelection::builtin(settings.fallback_theme);
            }
            Ok(settings.clone())
        }

        fn activate(&self, selection: &ThemeSelection) -> Result<ThemeSettings, ThemeStudioError> {
            let mut settings = self.settings.lock().unwrap();
            settings.active_theme = selection.clone();
            Ok(settings.clone())
        }
    }

    fn parse_json<T: for<'de> Deserialize<'de>>(reply: HttpReply) -> T {
        let mut bytes = Vec::new();
        let mut reader = reply.response.into_reader();
        reader.read_to_end(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn port_selection_returns_a_random_loopback_port() {
        let listeners = bind_theme_studio_listeners(STUDIO_HOST).unwrap();
        assert!(!listeners.is_empty());
        let port = listeners[0].local_addr().unwrap().port();
        assert!(port > 0);
    }

    #[test]
    fn get_state_returns_current_theme_state() {
        let store = MemoryThemeStore::new();
        let reply = route_request(&Method::Get, "/api/state", "", &store);
        let state: StateResponse = parse_json(reply);

        assert!(!state.builtins.is_empty());
        assert_eq!(state.custom_themes.len(), 1);
    }

    #[test]
    fn create_theme_returns_saved_theme_id() {
        let store = MemoryThemeStore::new();
        let payload = serde_json::to_string(&ThemeDraftPayload {
            id: None,
            name: "Neon".to_string(),
            palette: ThemePalette {
                panel: "#111111".to_string(),
                border: "#222222".to_string(),
                text: "#eeeeee".to_string(),
                muted: "#999999".to_string(),
                accent: "#00f5ff".to_string(),
                highlight: "#ff59c2".to_string(),
                success: "#6fffb1".to_string(),
                error: "#ff608d".to_string(),
            },
        })
        .unwrap();

        let response: SaveResponse = parse_json(route_request(
            &Method::Post,
            "/api/themes",
            &payload,
            &store,
        ));
        assert_eq!(response.saved_theme_id, "theme-2");
        assert_eq!(response.state.custom_themes.len(), 2);
    }

    #[test]
    fn activate_and_delete_routes_update_state() {
        let store = MemoryThemeStore::new();
        let activate_payload = serde_json::to_string(&ThemeSelection::custom("theme-1")).unwrap();
        let activate_response: ActivateResponse = parse_json(route_request(
            &Method::Post,
            "/api/activate",
            &activate_payload,
            &store,
        ));
        assert_eq!(
            activate_response.state.active_theme,
            ThemeSelection::custom("theme-1")
        );

        let delete_response: ActivateResponse = parse_json(route_request(
            &Method::Delete,
            "/api/themes/theme-1",
            "",
            &store,
        ));
        assert_eq!(
            delete_response.state.active_theme,
            ThemeSelection::builtin(ThemePreference::Dark)
        );
        assert!(delete_response.state.custom_themes.is_empty());
    }

    #[test]
    fn finish_route_requests_shutdown() {
        let store = MemoryThemeStore::new();
        let reply = route_request(&Method::Post, "/api/finish", "{}", &store);
        assert!(reply.should_finish);
    }
}
