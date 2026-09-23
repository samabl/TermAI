//! Byte-exact stdin -> stdout loopback helper used by the F0 fidelity tests.
//!
//! With a numeric argument N it echoes exactly N bytes and exits; without an
//! argument it copies until stdin reaches EOF. It is used both on ordinary pipes
//! (where std::io is byte exact) and on a unix pty slave.
#![deny(unsafe_op_in_unsafe_fn)]

use std::io::{Read, Write};

fn main() {
    let limit = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<usize>().ok());
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    match limit {
        Some(count) => {
            let mut buffer = vec![0u8; count];
            if input.read_exact(&mut buffer).is_ok() {
                let _ = output.write_all(&buffer);
                let _ = output.flush();
            }
        }
        None => {
            let mut buffer = [0u8; 4096];
            loop {
                match input.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if output.write_all(&buffer[..read]).is_err() {
                            break;
                        }
                        let _ = output.flush();
                    }
                }
            }
        }
    }
}
