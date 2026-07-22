//! A throwaway HTTP server standing in for an npm registry.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use serde_json::{Map, Value, json};

/// What the server answers for one path.
#[derive(Clone)]
pub struct Route {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Route {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
        }
    }

    pub fn json(document: &Value) -> Self {
        Self::ok(document.to_string())
    }

    pub fn status(status: u16) -> Self {
        Self {
            status,
            body: Vec::new(),
        }
    }
}

type Routes = Arc<Mutex<HashMap<String, Route>>>;

/// A registry that serves canned responses and records what was asked for.
pub struct TestRegistry {
    base_url: String,
    routes: Routes,
    requests: Arc<Mutex<Vec<String>>>,
}

impl TestRegistry {
    /// Binds a port and starts answering; routes are added afterwards so they
    /// can embed the server's own address.
    pub fn start() -> Self {
        let listener =
            TcpListener::bind("127.0.0.1:0").expect("the test registry should bind a port");
        let port = listener
            .local_addr()
            .expect("the listener should report its address")
            .port();

        let routes: Routes = Arc::new(Mutex::new(HashMap::new()));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let served = Arc::clone(&routes);
        let recorded = Arc::clone(&requests);

        // The thread is detached; it goes away when the test binary exits.
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                serve(stream, &served, &recorded);
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            routes,
            requests,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn serve(&self, path: &str, route: Route) {
        self.routes
            .lock()
            .expect("the route table should not be poisoned")
            .insert(path.to_string(), route);
    }

    /// Publishes a package's metadata and the tarball it points at.
    pub fn publish(&self, name: &str, version: &str, packument: &Value, tarball: Vec<u8>) {
        self.serve(&metadata_path(name), Route::json(packument));
        self.serve(&tarball_path(name, version), Route::ok(tarball));
    }

    /// Every path the server has been asked for, in order.
    pub fn requests(&self) -> Vec<String> {
        self.requests
            .lock()
            .expect("the request log should not be poisoned")
            .clone()
    }

    pub fn request_count(&self, path: &str) -> usize {
        self.requests()
            .iter()
            .filter(|request| *request == path)
            .count()
    }
}

fn serve(mut stream: TcpStream, routes: &Routes, requests: &Mutex<Vec<String>>) {
    let Some(path) = read_request_path(&stream) else {
        return;
    };

    requests
        .lock()
        .expect("the request log should not be poisoned")
        .push(path.clone());

    let route = routes
        .lock()
        .expect("the route table should not be poisoned")
        .get(&path)
        .cloned()
        .unwrap_or_else(|| Route::status(404));

    let header = format!(
        "HTTP/1.1 {} \r\nContent-Length: {}\r\nContent-Type: application/json\r\n\
         Connection: close\r\n\r\n",
        route.status,
        route.body.len()
    );

    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&route.body);
    let _ = stream.flush();
}

fn read_request_path(stream: &TcpStream) -> Option<String> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok()?;

    // Drain the headers so the client is not blocked writing them.
    let mut header = String::new();
    while reader.read_line(&mut header).ok()? > 2 {
        header.clear();
    }

    request_line.split_whitespace().nth(1).map(str::to_string)
}

/// Assembles the metadata document a registry publishes for one package.
pub struct PackumentBuilder {
    base_url: String,
    name: String,
    versions: Map<String, Value>,
    tags: Map<String, Value>,
}

impl PackumentBuilder {
    pub fn new(base_url: &str, name: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            name: name.to_string(),
            versions: Map::new(),
            tags: Map::new(),
        }
    }

    /// Adds a published version with the given dependency ranges.
    pub fn version(mut self, version: &str, dependencies: &[(&str, &str)]) -> Self {
        let dependencies: Map<String, Value> = dependencies
            .iter()
            .map(|(name, range)| ((*name).to_string(), json!(range)))
            .collect();

        self.versions.insert(
            version.to_string(),
            json!({
                "name": self.name,
                "version": version,
                "dependencies": dependencies,
                "dist": { "tarball": self.tarball_url(version) },
            }),
        );
        self
    }

    /// Replaces a version's `dist` block, keeping the tarball URL intact.
    pub fn with_dist_field(mut self, version: &str, field: &str, value: Value) -> Self {
        if let Some(entry) = self.versions.get_mut(version) {
            entry["dist"][field] = value;
        }
        self
    }

    /// Adds a `bin` declaration to an already added version.
    pub fn with_bin(mut self, version: &str, bin: Value) -> Self {
        if let Some(entry) = self.versions.get_mut(version) {
            entry["bin"] = bin;
        }
        self
    }

    pub fn tag(mut self, tag: &str, version: &str) -> Self {
        self.tags.insert(tag.to_string(), json!(version));
        self
    }

    pub fn build(self) -> Value {
        json!({
            "name": self.name,
            "dist-tags": self.tags,
            "versions": self.versions,
        })
    }

    fn tarball_url(&self, version: &str) -> String {
        format!("{}{}", self.base_url, tarball_path(&self.name, version))
    }
}

/// The path a package's metadata is served from, with the scope escaped.
pub fn metadata_path(name: &str) -> String {
    format!("/{}", name.replace('/', "%2F"))
}

/// The path a package's tarball is served from.
pub fn tarball_path(name: &str, version: &str) -> String {
    let stem = name.rsplit('/').next().unwrap_or(name);

    format!("/{name}/-/{stem}-{version}.tgz")
}
