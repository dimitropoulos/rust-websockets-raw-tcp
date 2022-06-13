extern crate base64;
mod error;
mod frame;
use crate::frame::{Data as OpData, Frame, OpCode};
use frame::{apply_mask, FrameHeader};
use sha1::{Digest, Sha1};
use std::io::Cursor;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::str::Lines;
use std::thread;

fn get_accept_key_header(lines: &mut Lines) -> Result<String, String> {
    let magic_string = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

    for line in lines {
        let fixed_line = line.to_string();
        if fixed_line.to_lowercase().contains("sec-websocket-key") {
            let (_, key) = fixed_line.split_at(19);

            let mut hasher = Sha1::new();
            hasher.update(key);
            hasher.update(magic_string);
            let sha1 = hasher.finalize();

            let b64 = base64::encode(&sha1);

            let output = format!("Sec-WebSocket-Accept: {b64}");
            return Ok(output);
        }
    }
    Err(String::from("Sec-Websocket-Key header not found"))
}

fn handshake_response(mut stream: &TcpStream) {
    let mut buffer = [0; 4096];
    stream.read(&mut buffer).unwrap();
    let request = String::from_utf8_lossy(&buffer[..]);
    let mut lines = request.lines();
    println!("{request}");
    let accept_key_header = get_accept_key_header(&mut lines).unwrap();

    let headers = [
        "HTTP/1.1 101 Switching Protocols",
        "Upgrade: websocket",
        "Connection: Upgrade",
        accept_key_header.as_str(),
        "Date: Sat, 28 May 2022 18:12:34 GMT",
        "\r\n",
    ];
    stream.write(&headers.join("\r\n").into_bytes()).ok();
}

fn handle_client(mut stream: TcpStream) {
    let mut data = [0_u8; 4096];
    while match stream.read(&mut data) {
        Ok(size) => {
            let mut raw: Cursor<Vec<u8>> = Cursor::new(data.into());

            let (header, length) = FrameHeader::parse(&mut raw).unwrap().unwrap();

            let mut payload = Vec::new();
            payload.resize(length as _, 0);
            raw.read_exact(&mut payload).unwrap();

            if let Some(mask) = header.mask {
                apply_mask(&mut payload, mask);
            }

            let frame = Frame::message(payload, OpCode::Data(OpData::Text));

            let mut out_buffer: Vec<u8> = Vec::new();
            frame
                .format(&mut out_buffer)
                .expect("can't write to vector");

            stream.write_all(&out_buffer).unwrap();
            stream.flush().unwrap();
            true
        }
        Err(_) => {
            println!(
                "An error occurred, terminating connection with {}",
                stream.peer_addr().unwrap()
            );
            stream.shutdown(Shutdown::Both).unwrap();
            false
        }
    } {}
}

fn main() {
    let listener = TcpListener::bind("0.0.0.0:3333").unwrap();
    // accept connections and process them, spawning a new thread for each one
    println!("Server listening on port 3333");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                handshake_response(&stream);

                thread::spawn(move || {
                    // connection succeeded
                    handle_client(stream)
                });
            }
            Err(error) => {
                /* connection failed */
                println!("Error: {}", error);
            }
        }
    }

    // close the socket server
    drop(listener);
}
