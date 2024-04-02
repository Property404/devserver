use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha1::{Digest, Sha1};
use std::{
    io::{Read, Write},
    net::{IpAddr, TcpListener},
    path::Path,
    str, thread,
    time::Duration,
};

pub const RELOAD_PORT: u16 = 8129; /* Arbitrary port */

fn parse_websocket_handshake(bytes: &[u8]) -> String {
    let request_string = str::from_utf8(bytes).unwrap();
    let lines = request_string.split("\r\n");
    let mut sec_websocket_key = "";

    for line in lines {
        let parts: Vec<&str> = line.split(':').collect();
        if let "Sec-WebSocket-Key" = parts[0] {
            sec_websocket_key = parts[1].trim();
        }
    }

    // Perform a ceremony of getting the SHA1 hash of the sec_websocket_key joined with
    // an arbitrary string and then take the base 64 encoding of that.
    let sec_websocket_accept = format!(
        "{}{}",
        sec_websocket_key, "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"
    );
    let mut hasher = Sha1::new();
    hasher.input(sec_websocket_accept.as_bytes());
    let result = hasher.result();
    let bytes = base64::encode(&result);

    format!("HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",bytes)
}

// This function can send strings of text to a websocket stream.
fn send_websocket_message<T: Write>(mut stream: T) -> Result<(), std::io::Error> {
    let payload_length = 0;

    stream.write_all(&[129])?; // Devserver always sends text messages. The combination of bitflags and opcode produces '129'
    let mut second_byte: u8 = 0;

    second_byte |= payload_length as u8;
    stream.write_all(&[second_byte])?;

    Ok(())
}

fn handle_websocket_handshake<T: Read + Write>(mut stream: T) {
    let header = crate::read_header(&mut stream);
    let response = parse_websocket_handshake(&header);
    stream.write_all(response.as_bytes()).unwrap();
    stream.flush().unwrap();
}

pub fn watch_for_reloads(address: IpAddr, path: &Path) {
    // Setup websocket receiver.
    let listener = TcpListener::bind((address, RELOAD_PORT)).unwrap();

    // The only incoming message we expect to receive is the initial handshake.
    for stream in listener.incoming() {
        let path = path.to_owned();

        thread::spawn(move || {
            if let Ok(mut stream) = stream {
                handle_websocket_handshake(&mut stream);

                // We do not handle ping/pong requests. Is that bad?
                // This code also assumes the client will never send any messages
                // other than the initial handshake.
                let (tx, rx) = std::sync::mpsc::channel();
                /* Is a 10ms delay here too short?*/
                let watcher_config = Config::default().with_poll_interval(Duration::from_secs(10));
                let mut watcher: RecommendedWatcher = Watcher::new(tx, watcher_config).unwrap();
                watcher.watch(&path, RecursiveMode::Recursive).unwrap();

                // Watch for file changes until the socket closes.
                loop {
                    match rx.recv() {
                        Ok(Ok(event)) => {
                            // Only refresh the web page for new and modified files
                            // For now do not refresh for removed or renamed files, but in the future
                            // it may be better to refresh then to immediately reflect path errors.
                            let refresh =
                                matches!(event.kind, EventKind::Create(..) | EventKind::Modify(..));

                            if refresh {
                                // A blank message is sent triggering a refresh on any file change.
                                // If this message fails to send, then likely the socket has been closed.
                                if send_websocket_message(&stream).is_err() {
                                    break;
                                };
                            }
                        }
                        Ok(Err(e)) => println!("File watch error: {:?}", e),
                        Err(e) => println!("File watch error: {:?}", e),
                    };
                }
            }
        });
    }
}
