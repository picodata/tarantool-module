use super::AsClient;
use crate::error::Error;
use crate::fiber::r#async::Mutex;
use crate::network::client::ClientError;
use crate::network::protocol;
use std::rc::Rc;
use std::sync::Arc;

#[cfg(feature = "internal_test")]
use std::sync::atomic::{AtomicUsize, Ordering};

type ClientOrConnectionClosedError = Result<super::Client, Arc<Error>>;

/// A reconnecting version of [`super::Client`].
///
/// Does not reconnect automatically but provides a method [`Client::reconnect`] for explicit reconnection,
/// when the user finds it necessary.
/// Can be cloned to utilize the same connection from multiple fibers.
///
/// See [`AsClient`] for the full API.
#[derive(Debug, Clone)]
pub struct Client {
    client: Rc<Mutex<Option<ClientOrConnectionClosedError>>>,
    url: String,
    port: u16,
    protocol_config: protocol::Config,

    // Testing related code
    #[cfg(feature = "internal_test")]
    inject_error: Rc<std::cell::RefCell<Option<ClientError>>>,
    #[cfg(feature = "internal_test")]
    reconnect_count: Rc<AtomicUsize>,
}

impl Client {
    /// Provides an access to the underlying client behind mutex.
    /// If it is `None` - reconnects implicitly and returns a new client.
    async fn client(&self) -> Result<super::Client, ClientError> {
        let mut client = self.client.lock().await;
        match &*client {
            Some(Ok(client)) => {
                return Ok(client.clone());
            }
            Some(Err(e)) => {
                return Err(ClientError::ConnectionClosed(e.clone()));
            }
            None => {}
        }

        #[cfg(feature = "internal_test")]
        {
            self.reconnect_count.fetch_add(1, Ordering::Relaxed);
        }

        let res =
            super::Client::connect_with_config(&self.url, self.port, self.protocol_config.clone())
                .await;
        match res {
            Ok(new_client) => {
                *client = Some(Ok(new_client.clone()));
                return Ok(new_client);
            }
            Err(ClientError::ConnectionClosed(e)) => {
                *client = Some(Err(e.clone()));
                return Err(ClientError::ConnectionClosed(e));
            }
            Err(_) => unreachable!(
                "Client::connect_with_config should only return `ConnectionClosed` errors"
            ),
        }
    }

    /// Request client to reconnect before executing next operation.
    ///
    /// If one of the cloned clients (used in other fibers/places) has already reconnected,
    /// this client will use this new connection instead of trying to establish a new one.
    ///
    /// When reconnection happens ongoing requests (processing in other fibers) will
    /// continue on the old connection, but any new request will use the new connection.
    pub fn reconnect(&self) {
        if let Some(mut client) = self.client.try_lock() {
            *client = None;
        } else {
            // if the lock is already captured, then the client is already in the process of reconnecting
        }
    }

    /// Force reconnection.
    ///
    /// If one of the cloned clients (used in other fibers/places) has already reconnected,
    /// this client will use this new connection instead of trying to establish a new one.
    ///
    /// When reconnection happens ongoing requests (processing in other fibers) will
    /// continue on the old connection, but any new request will use the new connection.
    ///
    /// # Errors
    /// Error is returned if reconnection fails.
    /// See [`Error`].
    pub async fn reconnect_now(&self) -> Result<(), Error> {
        self.reconnect();
        self.client().await?;
        Ok(())
    }

    /// Creates a new client but does not yet try to establish connection
    /// to `url:port`. This will happen at the first call through [`AsClient`] methods.
    pub fn new(url: String, port: u16) -> Self {
        Self::with_config(url, port, Default::default())
    }

    /// Creates a new client but does not yet try to establish connection
    /// to `url:port`. This will happen at the first call through [`AsClient`] methods.
    ///
    /// Takes explicit `config` in comparison to [`Self::new`]
    /// where default values are used.
    pub fn with_config(url: String, port: u16, config: protocol::Config) -> Self {
        Self {
            client: Default::default(),
            url,
            port,
            protocol_config: config,

            #[cfg(feature = "internal_test")]
            inject_error: Default::default(),
            #[cfg(feature = "internal_test")]
            reconnect_count: Default::default(),
        }
    }

    #[cfg(feature = "internal_test")]
    pub fn reconnect_count(&self) -> usize {
        // Don't count initial connection
        self.reconnect_count
            .load(Ordering::Relaxed)
            .saturating_sub(1)
    }
}

#[async_trait::async_trait(?Send)]
impl AsClient for Client {
    async fn send<R: protocol::api::Request>(
        &self,
        request: &R,
    ) -> Result<R::Response, ClientError> {
        let client = self.client().await?;

        #[cfg(not(feature = "internal_test"))]
        {
            client.send(request).await
        }
        // Allow error injection in tests
        #[cfg(feature = "internal_test")]
        {
            let inject_error = self.inject_error.borrow_mut().take();
            if let Some(error) = inject_error {
                Err(error)
            } else {
                client.send(request).await
            }
        }
    }
}

#[cfg(feature = "internal_test")]
mod tests {
    use super::*;
    use crate::fiber;
    use crate::fiber::r#async::timeout::IntoTimeout as _;
    use crate::test::util::listen_port;
    use std::time::Duration;

    const _3_SEC: Duration = Duration::from_secs(3);

    fn test_client() -> Client {
        Client::with_config(
            "localhost".into(),
            listen_port(),
            protocol::Config {
                creds: Some(("test_user".into(), "password".into())),
                auth_method: crate::auth::AuthMethod::ChapSha1,
                ..Default::default()
            },
        )
    }

    #[crate::test(tarantool = "crate")]
    async fn connect_failure() {
        // Can be any other unused port
        let client = Client::new("localhost".into(), 0);
        let err = client.ping().await.unwrap_err();
        let correct_err = [
            "failed to connect to address 'localhost:0': Connection refused (os error 111)",
            "failed to connect to address 'localhost:0': Cannot assign requested address (os error 99)",
            "failed to connect to address 'localhost:0': Can't assign requested address (os error 49)",
        ].contains(dbg!(&&*err.to_string()));
        assert!(correct_err);
    }

    #[crate::test(tarantool = "crate")]
    async fn ping_after_reconnect() {
        let client = test_client();

        for _ in 0..2 {
            client.ping().timeout(_3_SEC).await.unwrap();
        }
        assert_eq!(client.reconnect_count(), 0);
        client.reconnect();
        for _ in 0..2 {
            client.ping().timeout(_3_SEC).await.unwrap();
        }
        assert_eq!(client.reconnect_count(), 1);
    }

    #[crate::test(tarantool = "crate")]
    async fn reconnect_now_vs_later() {
        let client = test_client();
        // Client initializes at initial request
        client.ping().timeout(_3_SEC).await.unwrap();
        assert_eq!(client.reconnect_count(), 0);

        // Reconnect happens at the first send
        client.reconnect();
        assert_eq!(client.reconnect_count(), 0);
        client.ping().timeout(_3_SEC).await.unwrap();
        assert_eq!(client.reconnect_count(), 1);

        // Reconnect happens right away
        client.reconnect_now().await.unwrap();
        assert_eq!(client.reconnect_count(), 2);
    }

    // More of an example of how this client can be used than a test
    #[crate::test(tarantool = "crate")]
    fn reconnect_on_network_error() {
        use std::io::{Error as IOError, ErrorKind};
        use std::sync::Arc;
        fiber::block_on(async {
            let client = test_client();

            let err = ClientError::ConnectionClosed(Arc::new(
                IOError::from(ErrorKind::ConnectionAborted).into(),
            ));
            *client.inject_error.borrow_mut() = Some(err);
            client.ping().timeout(_3_SEC).await.unwrap_err();
            client.reconnect_now().await.unwrap();
            assert_eq!(client.reconnect_count(), 1);

            let err = ClientError::ConnectionClosed(Arc::new(
                IOError::from(ErrorKind::ConnectionAborted).into(),
            ));
            *client.inject_error.borrow_mut() = Some(err);
            client.ping().timeout(_3_SEC).await.unwrap_err();
            client.reconnect_now().await.unwrap();
            assert_eq!(client.reconnect_count(), 2);
        });
    }

    #[crate::test(tarantool = "crate")]
    fn old_connection_remains_for_old_request() {
        let lua = crate::global_lua();
        lua.exec(
            "fiber = require('fiber')
            _G.reconnect_test_chan = fiber.channel()",
        )
        .unwrap();
        let client = test_client();
        fiber::block_on(client.ping()).unwrap();
        assert_eq!(client.reconnect_count(), 0);
        let client_clone = client.clone();
        let jh = fiber::defer_async(async move {
            client_clone.reconnect_now().await.unwrap();
            assert_eq!(client_clone.reconnect_count(), 1);
            lua.exec("_G.reconnect_test_chan:put(42)").unwrap();
        });
        fiber::block_on(async move {
            // reconnect will happen during this request
            let result = client
                .eval("return _G.reconnect_test_chan:get()", &())
                .await
                .unwrap()
                .decode::<(i32,)>()
                .unwrap();
            // value received on an old connection, though there was a reconnect request
            assert_eq!(result, (42,));
            // Globally the client has 1 reconnection
            assert_eq!(client.reconnect_count(), 1);
        });
        jh.join();
    }

    #[crate::test(tarantool = "crate")]
    async fn concurrent_messages_one_fiber() {
        let client = test_client();
        let mut ping_futures = vec![];
        for _ in 0..10 {
            ping_futures.push(client.ping());
        }
        for res in futures::future::join_all(ping_futures).await {
            res.unwrap();
        }
    }

    #[crate::test(tarantool = "crate")]
    async fn try_reconnect_only_once() {
        let client = Client::new("localhost".into(), 0);
        client.ping().await.unwrap_err();
        assert_eq!(client.reconnect_count(), 0);

        // If reconnect was requested once - try to reconnect only once
        // even if reconnection fails
        client.reconnect();
        for _ in 0..10 {
            client.ping().await.unwrap_err();
        }
        assert_eq!(client.reconnect_count(), 1);
    }
}
