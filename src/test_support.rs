use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use serde_json::Value;

pub struct Response {
    status: u16,
    reason: &'static str,
    headers: Vec<(String, String)>,
    body: String,
}

impl Response {
    pub fn json(body: Value) -> Self {
        Self {
            status: 200,
            reason: "OK",
            headers: vec![("Content-Type".into(), "application/json".into())],
            body: body.to_string(),
        }
    }

    pub fn status(status: u16, reason: &'static str) -> Self {
        Self {
            status,
            reason,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn body(mut self, body: &str) -> Self {
        self.body = body.into();
        self
    }
}

#[derive(Debug)]
pub struct Request {
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

impl Request {
    pub fn method(&self) -> &str {
        self.body["method"].as_str().expect("request method")
    }

    pub fn arguments(&self) -> &Value {
        &self.body["arguments"]
    }
}

pub struct ScriptedServer {
    pub url: String,
    requests: Receiver<Request>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ScriptedServer {
    pub fn start(responses: Vec<Response>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind scripted server");
        let address = listener.local_addr().expect("scripted server address");
        listener
            .set_nonblocking(true)
            .expect("configure scripted server");
        let (tx, requests) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            for response in responses {
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            if thread_stop.load(Ordering::Relaxed) {
                                return;
                            }
                            thread::sleep(std::time::Duration::from_millis(2));
                        }
                        Err(error) => panic!("accept scripted request: {error}"),
                    }
                };
                stream
                    .set_nonblocking(false)
                    .expect("configure scripted request stream");
                let request = read_request(stream.try_clone().expect("clone request stream"));
                if tx.send(request).is_err() {
                    return;
                }
                write_response(stream, response);
            }
        });
        Self {
            url: format!("http://{address}/transmission/rpc"),
            requests,
            stop,
            thread: Some(thread),
        }
    }

    pub fn request(&self) -> Request {
        self.requests
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("scripted request was not received")
    }
}

impl Drop for ScriptedServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn read_request(stream: TcpStream) -> Request {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).expect("request line");
    assert!(request_line.starts_with("POST "), "{request_line:?}");

    let mut headers = BTreeMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).expect("request header");
        if line == "\r\n" || line.is_empty() {
            break;
        }
        let (name, value) = line.split_once(':').expect("valid request header");
        headers.insert(name.to_ascii_lowercase(), value.trim().to_string());
    }

    let length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("request body");
    Request {
        headers,
        body: serde_json::from_slice(&body).expect("JSON request body"),
    }
}

fn write_response(mut stream: TcpStream, response: Response) {
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.reason,
        response.body.len()
    )
    .expect("response status");
    for (name, value) in response.headers {
        write!(stream, "{name}: {value}\r\n").expect("response header");
    }
    write!(stream, "\r\n{}", response.body).expect("response body");
}

pub fn success(arguments: Value) -> Response {
    Response::json(serde_json::json!({
        "result": "success",
        "arguments": arguments,
    }))
}
