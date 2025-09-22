//! Tarantool Binary Protocol implementation without actual transport layer.
//!
//! According to Sans-I/O pattern this implementation processes requests and responses
//! by outputing the corresponding bytes into incoming and outgoing buffers.
//! And it provides an API for the upper client layer to get data from these buffers.
//!
//! See [`super::client`] if you need a fully functional Tarantool client.

pub mod api;
pub use api::*;
pub mod codec;
pub use codec::*;

use crate::auth::AuthMethod;
use crate::error;
use crate::error::TarantoolError;
use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use std::time::Duration;

#[deprecated = "use `ProtocolError` instead"]
pub type Error = ProtocolError;

/// IProto protocol violation.
#[non_exhaustive]
#[derive(thiserror::Error, Debug)]
pub enum ProtocolError {
    #[error("message size hint is 0")]
    ZeroSizeHint,

    #[error("{key} not found in iproto response body, {context}")]
    ResponseFieldNotFound {
        key: &'static str,
        context: &'static str,
    },

    #[error("{0} is not implemented yet")]
    Unimplemented(String),
}

/// Unique identifier of the sent message on this connection.
/// It is used to retrieve response for the corresponding request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SyncIndex(pub(crate) u64);

impl SyncIndex {
    /// Increments current sync value.
    pub fn next_index(&mut self) -> Self {
        let sync = self.0;
        self.0 += 1;
        Self(sync)
    }

    /// Returns current sync value.
    #[inline(always)]
    pub fn get(&self) -> u64 {
        self.0
    }
}

#[deprecated = "use `TarantoolError` instead"]
pub type ResponseError = TarantoolError;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum State {
    /// Awaits greeting
    Init,
    /// Awaits ID response
    Id,
    /// Awaits auth
    Auth,
    /// Ready to accept new messages
    Ready,
}

/// Configuration of [`Protocol`].
#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Config {
    /// (user, password)
    pub creds: Option<(String, String)>,
    /// Authentication method. Only useful in picodata.
    pub auth_method: AuthMethod,
    /// Connection establishment timeout.
    pub connect_timeout: Option<Duration>,
    /// Optional cluster uuid to pass via IPROTO_ID after auth.
    pub cluster_uuid: Option<String>,
    // TODO: add buffer limits here
}

/// A sans-io connection handler.
///
/// Buffers incoming and outgoing bytes and provides an API for
/// a client implementation to:
/// - Input requests
/// - Get processed responses
/// - Retrieve outgoing bytes
/// - Input incoming bytes
#[derive(Debug)]
pub struct Protocol {
    state: State,
    msg_size_hint: Option<usize>,
    outgoing: Vec<u8>,
    pending_outgoing: Vec<u8>,
    sync: SyncIndex,
    // TODO: limit incoming size
    incoming: HashMap<SyncIndex, Result<Vec<u8>, TarantoolError>>,
    /// (user, password)
    creds: Option<(String, String)>,
    auth_method: AuthMethod,
    // Cluster uuid to send via IPROTO_ID when connection becomes ready.
    cluster_uuid: Option<String>,
    /// Greeting salt captured from server greeting to be used for Auth after ID.
    greeting_salt: Option<Vec<u8>>,
}

impl Default for Protocol {
    fn default() -> Self {
        Self::new()
    }
}

impl Protocol {
    /// Construct [`Protocol`] with default values for [`Config`].
    pub fn new() -> Self {
        Self {
            state: State::Init,
            sync: SyncIndex(0),
            pending_outgoing: Vec::new(),
            creds: None,
            auth_method: AuthMethod::default(),
            outgoing: Vec::new(),
            incoming: HashMap::new(),
            // Greeting is exactly 128 bytes
            msg_size_hint: Some(128),
            cluster_uuid: None,
            greeting_salt: None,
        }
    }

    /// Construct [`Protocol`] with custom values for [`Config`].
    pub fn with_config(config: Config) -> Self {
        let mut protocol = Self::new();
        protocol.creds = config.creds;
        protocol.auth_method = config.auth_method;
        protocol.cluster_uuid = config.cluster_uuid;
        protocol
    }

    /// Returns `true` if the [`Protocol`] has passed initialization and authorization
    /// stages.
    ///
    /// Data can be sent independently of whether the protocol [`Self::is_ready`].
    /// If the protocol is not ready data will be queued and eventually processed
    /// after auth is done.
    pub fn is_ready(&self) -> bool {
        matches!(self.state, State::Ready)
    }

    /// Processes incoming request and buffers generated outgoing bytes.
    /// Outgoing bytes can be retrieved with [`Protocol::take_outgoing_data`]
    ///
    /// Data can be sent independently of whether the protocol [`Self::is_ready`].
    /// If the protocol is not ready data will be queued and eventually processed
    /// after auth is done.
    pub fn send_request(&mut self, request: &impl Request) -> Result<SyncIndex, error::Error> {
        let end = self.pending_outgoing.len();
        let mut buf = Cursor::new(&mut self.pending_outgoing);
        buf.set_position(end as u64);
        // TODO: limit the pending vec size
        // FIXME: Theoretically an error can happen in `Request::encode`.
        // This shouldn't ever happen in practice, as we're just writing into a memory buffer,
        // but our interfaces allow for this. So in case this happens here we will likely end
        // up with corrupted data in `self.pending_outgoing`.
        // It's pretty easy to fix, so we probably should...
        write_to_buffer(&mut buf, self.sync, request)?;
        self.process_pending_data();
        Ok(self.sync.next_index())
    }

    /// Take existing response by [`SyncIndex`].
    pub fn take_response<R: Request>(
        &mut self,
        sync: SyncIndex,
    ) -> Option<Result<R::Response, error::Error>> {
        let response = match self.incoming.remove(&sync)? {
            Ok(response) => response,
            Err(err) => return Some(Err(error::Error::Remote(err))),
        };
        Some(R::decode_response_body(&mut Cursor::new(response)))
    }

    /// Drop response by [`SyncIndex`] if it exists. If not - does nothing.
    pub fn drop_response(&mut self, sync: SyncIndex) {
        self.incoming.remove(&sync);
    }

    /// See [`Protocol::process_incoming`].
    pub fn read_size_hint(&self) -> usize {
        if let Some(hint) = self.msg_size_hint {
            hint
        } else {
            // Reading the U32 message size hint
            // Read 5 bytes, 1st is a marker
            5
        }
    }

    /// Processes incoming bytes received over transport layer.
    ///
    /// Should be used together with [`Protocol::read_size_hint`] e.g:
    /// 1. Call `read_size_hint` and get its value.
    ///    It is the number of bytes a client implementation should read from transport.
    /// 2. Read the required number of bytes from transport
    /// 3. Call [`Protocol::process_incoming`] with these bytes.
    ///
    /// Returns a [`SyncIndex`] if non-technical message was received.
    /// This message can be retreived by this index with [`Protocol::take_response`].
    pub fn process_incoming<R: Read + Seek>(
        &mut self,
        chunk: &mut R,
    ) -> Result<Option<SyncIndex>, error::Error> {
        if self.msg_size_hint.is_some() {
            // Message size hint was already read at previous call - now processing message
            self.msg_size_hint = None;
            self.process_message(chunk)
        } else {
            // Message was read at previous call - now reading size hint
            let hint = rmp::decode::read_u32(chunk)?;
            if hint > 0 {
                self.msg_size_hint = Some(hint as usize);
                Ok(None)
            } else {
                Err(ProtocolError::ZeroSizeHint.into())
            }
        }
    }

    /// Handle error response and return appropriate error
    fn handle_error_response<R: Read + Seek>(
        &self,
        message: &mut R,
        header: &codec::Header,
    ) -> Result<(), error::Error> {
        if header.iproto_type == IProtoType::Error as u32 {
            let error = codec::decode_error(message, header)?;
            return Err(error::Error::Remote(error));
        }
        Ok(())
    }

    /// Send auth request to the server
    fn send_auth_request(
        &mut self,
        user: &str,
        pass: &str,
        salt: &[u8],
    ) -> Result<(), error::Error> {
        debug_assert!(self.outgoing.is_empty());
        let mut buf = Cursor::new(&mut self.outgoing);
        let sync = self.sync.next_index();
        write_to_buffer(
            &mut buf,
            sync,
            &api::Auth {
                user,
                pass,
                salt,
                method: self.auth_method,
            },
        )
    }

    /// Send ID request to the server
    fn send_id_request(&mut self) -> Result<(), error::Error> {
        debug_assert!(self.outgoing.is_empty());
        let mut buf = Cursor::new(&mut self.outgoing);
        let sync = self.sync.next_index();
        write_to_buffer(
            &mut buf,
            sync,
            &api::Id {
                cluster_uuid: self.cluster_uuid.as_deref(),
            },
        )
    }

    fn process_message<R: Read + Seek>(
        &mut self,
        message: &mut R,
    ) -> Result<Option<SyncIndex>, error::Error> {
        let sync = match self.state {
            State::Init => {
                let salt = codec::decode_greeting(message)?;
                self.greeting_salt = Some(salt.clone());
                if self.cluster_uuid.is_some() {
                    self.state = State::Id;
                    self.send_id_request()?;
                } else if let Some((user, pass)) = self.creds.clone() {
                    // Auth
                    self.state = State::Auth;
                    self.send_auth_request(&user, &pass, &salt)?;
                } else {
                    // No auth
                    self.state = State::Ready;
                }
                None
            }
            State::Id => {
                // Decode ID response. If it's an error, ignore only ER_INVALID_MSGPACK.
                // In vanilla Tarantool the ID body is type-checked against iproto_key_type;
                // if a key (e.g. CLUSTER_UUID) is missing there, the expected type resolves to 0
                // and mismatches the actual MP_STR, which triggers ER_INVALID_MSGPACK. Propagate
                // any other error.
                let header = codec::Header::decode(message)?;
                if header.iproto_type == IProtoType::Error as u32 {
                    let err = codec::decode_error(message, &header)?;
                    // 20 == ER_INVALID_MSGPACK
                    if err.code != 20 {
                        return Err(error::Error::Remote(err));
                    }
                    crate::say_warn!(
                        "IPROTO_ID: ignoring ER_INVALID_MSGPACK (code 20); vanilla Tarantool likely lacks iproto_key_type entry for CLUSTER_UUID"
                    );
                }

                if let Some((user, pass)) = self.creds.clone() {
                    self.state = State::Auth;
                    let salt = self.greeting_salt.clone().unwrap_or_default();
                    self.send_auth_request(&user, &pass, &salt)?;
                } else {
                    self.state = State::Ready;
                }
                None
            }
            State::Auth => {
                let header = codec::Header::decode(message)?;
                self.handle_error_response(message, &header)?;
                self.state = State::Ready;
                None
            }
            State::Ready => {
                let header = codec::Header::decode(message)?;
                let response = if header.iproto_type == IProtoType::Error as u32 {
                    Err(codec::decode_error(message, &header)?)
                } else {
                    // FIXME: we know the exact size of the body at this point
                    let mut buf = Vec::new();
                    message.read_to_end(&mut buf)?;
                    Ok(buf)
                };
                self.incoming.insert(header.sync, response);
                Some(header.sync)
            }
        };
        self.process_pending_data();
        Ok(sync)
    }

    /// Returns a number of outgoing data bytes.
    pub fn ready_outgoing_len(&self) -> usize {
        self.outgoing.len()
    }

    /// Returns buffered outgoing data leaving the buffer empty.
    ///
    /// The returned bytes can then be sent through a
    /// transport layer to a Tarantool server.
    pub fn take_outgoing_data(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outgoing)
    }

    fn process_pending_data(&mut self) {
        if self.is_ready() {
            let mut pending_data = std::mem::take(&mut self.pending_outgoing);
            // TODO: limit the ready vec size
            self.outgoing.append(&mut pending_data);
        }
    }
}

pub(crate) fn write_to_buffer(
    buffer: &mut Cursor<&mut Vec<u8>>,
    sync: SyncIndex,
    request: &impl Request,
) -> Result<(), error::Error> {
    // write MSG_SIZE placeholder
    let msg_start_offset = buffer.position();
    rmp::encode::write_u32(buffer, 0)?;

    // write message payload
    let payload_start_offset = buffer.position();
    request.encode(buffer, sync)?;
    let payload_end_offset = buffer.position();

    // calculate and write MSG_SIZE
    buffer.set_position(msg_start_offset);
    rmp::encode::write_u32(buffer, (payload_end_offset - payload_start_offset) as u32)?;
    buffer.set_position(payload_end_offset);

    Ok(())
}

// Tests have to be run in Tarantool environment due to `ToTupleBuffer` using `crate::Error` which contains `LuaError`
// and therefore lua symbols
#[cfg(feature = "internal_test")]
mod tests {
    use super::*;

    /// See [tarantool docs](https://www.tarantool.io/en/doc/latest/dev_guide/internals/iproto/authentication/#greeting-message).
    fn fake_greeting() -> Vec<u8> {
        let mut greeting = Vec::new();
        greeting.extend([0; 63].iter());
        greeting.push(b'\n');
        greeting.extend(b"QK2HoFZGXTXBq2vFj7soCsHqTo6PGTF575ssUBAJLAI=".iter());
        while greeting.len() < 127 {
            greeting.push(0);
        }
        greeting.push(b'\n');
        greeting
    }

    #[crate::test(tarantool = "crate")]
    fn connection_established() {
        let mut conn = Protocol::new();
        assert!(!conn.is_ready());
        assert_eq!(conn.msg_size_hint, Some(128));
        assert_eq!(conn.read_size_hint(), 128);
        conn.process_incoming(&mut Cursor::new(fake_greeting()))
            .unwrap();
        assert_eq!(conn.msg_size_hint, None);
        assert_eq!(conn.read_size_hint(), 5);
        assert!(conn.is_ready())
    }

    #[crate::test(tarantool = "crate")]
    fn send_bytes_generated() {
        let mut conn = Protocol::new();
        conn.process_incoming(&mut Cursor::new(fake_greeting()))
            .unwrap();
        conn.send_request(&api::Ping).unwrap();
        assert!(conn.ready_outgoing_len() > 0);
    }
}
