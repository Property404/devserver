/// A local host only for serving static files.
/// Simple and easy, but not robust or tested.
use native_tls::{Identity, Protocol, TlsAcceptor};
use std::sync::Arc;

use std::ffi::OsStr;
use std::fmt::Display;
use std::fs;
use std::io::BufRead;
use std::io::{Read, Write};
use std::net::{IpAddr, TcpListener};
use std::path::Path;
use std::str;
use std::thread;

mod mime;
mod reload;

pub type Action = Box<dyn Fn() -> Result<(), Box<dyn Display>> + Send>;

pub fn read_header<T: Read + Write>(stream: &mut T) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut reader = std::io::BufReader::new(stream);
    loop {
        reader.read_until(b'\n', &mut buffer).unwrap();
        // Read until end of header.
        if buffer.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    buffer
}

#[allow(unused)]
fn handle_client<T: Read + Write>(
    mut stream: T,
    root_path: impl AsRef<Path>,
    reload: bool,
    headers: &str,
) {
    let buffer = read_header(&mut stream);
    let request_string = str::from_utf8(&buffer).unwrap();

    if request_string.is_empty() {
        return;
    }

    // Split the request into different parts.
    let mut parts = request_string.split(' ');

    let _method = parts.next().unwrap().trim();
    let mut path = parts.next().unwrap().trim();
    let _http_version = parts.next().unwrap().trim();

    // Trim parameters from URL
    if let Some(parameters_index) = path.find('?') {
        path = &path[..parameters_index];
    }

    // Replace white space characters with proper whitespace and remove any paths that refer to the parent.
    let path = path.replace("../", "").replace("%20", " ");
    let path = if path.ends_with('/') {
        root_path.as_ref().join(Path::new(&format!(
            "{}{}",
            path.trim_start_matches('/'),
            "index.html"
        )))
    } else {
        root_path.as_ref().join(path.trim_matches('/'))
    };

    let extension = path.extension().and_then(OsStr::to_str);

    let (file_contents, extension) = if extension.is_some() {
        (fs::read(&path), extension)
    } else {
        // If the request has no extension look first for a matching file without an extension
        if let Ok(file_contents) = fs::read(&path) {
            println!("WARNING: Serving file without extension: [ {} ] with media type 'application/octet-stream'", &path.to_str().unwrap());
            (Ok(file_contents), None)
        } else {
            // If no file without an extension is found see if there's a file with a ".html" extension
            // This enables "pretty URLs" without a trailing `/` like: `example.com/blog-post`
            let file = fs::read(path.with_extension("html"));
            (file, Some("html"))
        }
    };

    if let Ok(mut file_contents) = file_contents {
        // Pair the file extension to a media (also known as MIME) type.
        let content_type = mime::from_extension(extension);

        #[allow(unused_mut)]
        let mut content_length = file_contents.len();

        // Prepare to inject code into HTML if reload is enabled.
        let reload_append = include_bytes!("reload.html");
        if extension == Some("html") && reload {
            content_length += reload_append.len();
        }

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-type: {}\r\nContent-Length: {}{}\r\n\r\n",
            content_type, content_length, headers
        );

        let mut bytes = response.as_bytes().to_vec();
        bytes.append(&mut file_contents);
        stream.write_all(&bytes).unwrap();

        // Inject code into HTML if reload is enabled
        if extension == Some("html") && reload {
            // Insert javascript for reloading
            stream.write_all(reload_append).unwrap();
        }

        stream.flush().unwrap();
    } else {
        println!("Could not find file: {}", path.to_str().unwrap());
        let response = "HTTP/1.1 404 NOT FOUND\r\n\r\n";
        stream.write_all(response.as_bytes()).unwrap();
        stream.flush().unwrap();
    }
}

pub fn run(
    address: IpAddr,
    port: u16,
    path: impl AsRef<Path>,
    reload: bool,
    headers: &str,
    actions: Vec<Action>,
) {
    let acceptor = {
        // Hard coded certificate generated with the following commands:
        // openssl req -x509 -newkey rsa:2048 -keyout key.pem -out cert.pem -days 36500 -nodes -subj "/"
        // openssl pkcs12 -export -out identity.pfx -inkey key.pem -in cert.pem
        // password for second command: 'debug'
        let bytes = include_bytes!("identity.pfx");
        let identity = Identity::from_pkcs12(bytes, "debug").unwrap();
        Arc::new(
            TlsAcceptor::builder(identity)
                .min_protocol_version(Some(Protocol::Tlsv12))
                .build()
                .unwrap(),
        )
    };

    {
        let path = path.as_ref().to_owned();
        thread::spawn(move || {
            let path = path.clone();
            reload::watch_for_reloads(address, &path, actions);
        });
    }

    let listener = TcpListener::bind((address, port)).unwrap();
    for stream in listener.incoming().flatten() {
        let acceptor = acceptor.clone();

        let path = path.as_ref().to_owned();
        let headers = headers.to_owned();
        thread::spawn(move || {
            // HTTP requests always begin with a verb like 'GET'.
            // HTTPS requests begin with a number, so peeking and checking for a number
            // is used to determine if a request is HTTPS or HTTP
            let mut buf = [0; 2];
            stream.peek(&mut buf).expect("peek failed");

            let is_https = !((buf[0] as char).is_alphabetic() && (buf[1] as char).is_alphabetic());

            if is_https {
                // acceptor.accept will block indefinitely if called with an HTTP stream.
                if let Ok(stream) = acceptor.accept(stream) {
                    handle_client(stream, &path, reload, &headers);
                }
            } else {
                handle_client(stream, &path, reload, &headers);
            }
        });
    }
}
