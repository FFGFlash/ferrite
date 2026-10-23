//! The LSP base protocol: `Content-Length` headers followed by a JSON body.

use std::io::{self, BufRead, Read, Write};

/// Largest message body accepted from a server.
pub const MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
/// Largest header block accepted from a server.
const MAX_HEADER_BYTES: u64 = 8 * 1024;

fn invalid(message: impl Into<String>) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Reads one message body, or `Ok(None)` at a clean end of the stream.
pub fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
  let mut length = None;
  let mut header_bytes = 0;
  let mut line = String::new();
  loop {
    line.clear();
    let read = reader
      .by_ref()
      .take(MAX_HEADER_BYTES + 1 - header_bytes)
      .read_line(&mut line)?;
    if read == 0 {
      return if header_bytes == 0 {
        Ok(None)
      } else {
        Err(io::ErrorKind::UnexpectedEof.into())
      };
    }
    header_bytes += read as u64;
    if header_bytes > MAX_HEADER_BYTES {
      return Err(invalid("message header is too long"));
    }
    let header = line.trim_end_matches(['\r', '\n']);
    if header.is_empty() {
      if length.is_some() {
        break;
      }
      // Tolerate stray blank lines between messages.
      continue;
    }
    let (name, value) = header
      .split_once(':')
      .ok_or_else(|| invalid(format!("malformed header: {header}")))?;
    if name.trim().eq_ignore_ascii_case("content-length") {
      let value: usize = value
        .trim()
        .parse()
        .map_err(|_| invalid(format!("invalid Content-Length: {value}")))?;
      length = Some(value);
    }
  }
  let length = length.ok_or_else(|| invalid("missing Content-Length"))?;
  if length > MAX_MESSAGE_BYTES {
    return Err(invalid(format!("message of {length} bytes is too large")));
  }
  let mut body = vec![0; length];
  reader.read_exact(&mut body)?;
  Ok(Some(body))
}

/// Writes one message.
pub fn write_message(writer: &mut impl Write, body: &[u8]) -> io::Result<()> {
  write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
  writer.write_all(body)?;
  writer.flush()
}

#[cfg(test)]
mod tests {
  use std::io::BufReader;

  use super::*;

  #[test]
  fn round_trips_messages() {
    let mut stream = Vec::new();
    write_message(&mut stream, br#"{"a":1}"#).expect("writes");
    write_message(&mut stream, "{\"é\":2}".as_bytes()).expect("writes");
    let mut reader = BufReader::new(stream.as_slice());
    assert_eq!(
      read_message(&mut reader).expect("reads").as_deref(),
      Some(br#"{"a":1}"#.as_slice())
    );
    assert_eq!(
      read_message(&mut reader).expect("reads").as_deref(),
      Some("{\"é\":2}".as_bytes())
    );
    assert_eq!(read_message(&mut reader).expect("reads"), None);
  }

  #[test]
  fn accepts_other_headers_and_any_case() {
    let input = b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{}";
    let mut reader = BufReader::new(input.as_slice());
    assert_eq!(
      read_message(&mut reader).expect("reads").as_deref(),
      Some(b"{}".as_slice())
    );
  }

  #[test]
  fn rejects_bad_input() {
    let read = |input: &[u8]| read_message(&mut BufReader::new(input));
    assert!(read(b"Content-Length: x\r\n\r\n").is_err());
    assert!(read(b"Content-Type: a\r\n\r\n{}").is_err(), "no length");
    assert!(
      read(b"Content-Length: 10\r\n\r\n{}").is_err(),
      "truncated body"
    );
    assert!(read(format!("Content-Length: {}\r\n\r\n", MAX_MESSAGE_BYTES + 1).as_bytes()).is_err());
    let endless = vec![b'x'; 10_000];
    assert!(read(&endless).is_err(), "header without end");
  }
}
