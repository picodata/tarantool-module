use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use super::Error;
use crate::fiber::r#async::watch;
use crate::network::protocol;

/// A reconnecting version of [`super::Client`].
///
/// Reconnects during requests either on timeout or when network error happens.
/// Can be cloned to utilize the same connection from multiple fibers.
///
/// See [`super::AsClient`] for the full API.
#[derive(Debug, Clone)]
pub struct Client {
    // Client is optional here as con construction it is `None`
    // but it will always be `Some` after the first `handle_reconnect` call
    client: RefCell<Option<super::Client>>,
    new_client_rx: RefCell<watch::Receiver<Option<super::Client>>>,
    new_client_tx: Rc<RefCell<watch::Sender<Option<super::Client>>>>,
    should_reconnect: Cell<bool>,
    url: String,
    port: u16,
    protocol_config: protocol::Config,

    // Testing related code
    #[cfg(feature = "internal_test")]
    inject_error: Rc<RefCell<Option<super::Error>>>,
}

impl Client {
    async fn handle_reconnect(&self) -> Result<(), Error> {
        let has_changed = self.new_client_rx.borrow().has_changed();
        // Send new messages over new connection if it exists
        if has_changed {
            self.new_client_rx.borrow_mut().mark_seen();
            *self.client.borrow_mut() = self.new_client_rx.borrow().get_cloned();
        // Reconnect if asked and it didn't already happen on other client clones
        } else if self.should_reconnect.get() {
            let new_client = super::Client::connect_with_config(
                &self.url,
                self.port,
                self.protocol_config.clone(),
            )
            .await?;
            *self.client.borrow_mut() = Some(new_client);
            self.new_client_tx
                .borrow_mut()
                .send(self.client.borrow().clone())
                .expect("no references should be held");
            self.new_client_rx.borrow_mut().mark_seen();
        }
        self.should_reconnect.set(false);
        Ok(())
    }

    /// Request client to reconnect before executing next operation.
    ///
    /// If one of the cloned clients (used in other fibers/places) has already reconnected,
    /// this client will use this new connection instead of trying to establish a new one.
    ///
    /// When reconnection happens ongoing requests (processing in other fibers) will
    /// continue on the old connection, but any new request will use the new connection.
    pub fn reconnect(&self) {
        self.should_reconnect.set(true);
    }

    /// Force reconnection.
    ///
    /// If one of the cloned clients (used in other fibers/places) has already reconnected,
    /// this client will use this new connection instead of trying to establish a new one.
    ///
    /// When reconnection happens ongoing requests (processing in other fibers) will
    /// continue on the old connection, but any new request will use the new connection.
    pub async fn reconnect_now(&self) -> Result<(), Error> {
        self.reconnect();
        self.handle_reconnect().await
    }

    /// Creates a new client but does not yet try to establish connection
    /// to `url:port`. This will happen at the first call through [`super::AsClient`] methods.
    ///
    /// # Errors
    /// Error is returned if an attempt to connect failed.
    /// See [`Error`].
    pub fn new(url: String, port: u16) -> Self {
        Self::with_config(url, port, Default::default())
    }

    /// Creates a new client but does not yet try to establish connection
    /// to `url:port`. This will happen at the first call through [`AsClient`] methods.
    ///
    /// Takes explicit `config` in comparison to [`Client::connect`]
    /// where default values are used.
    ///
    /// # Errors
    /// Error is returned if an attempt to connect failed.
    /// See [`Error`].
    fn with_config(url: String, port: u16, config: protocol::Config) -> Self {
        let (new_client_tx, new_client_rx) = watch::channel(None);
        Self {
            client: RefCell::new(None),
            url,
            port,
            protocol_config: config,
            new_client_rx: RefCell::new(new_client_rx),
            new_client_tx: Rc::new(RefCell::new(new_client_tx)),

            #[cfg(feature = "internal_test")]
            inject_error: Default::default(),
            should_reconnect: Cell::new(true),
        }
    }

    #[cfg(feature = "internal_test")]
    pub fn inject_error(&self, error: super::Error) {
        *self.inject_error.borrow_mut() = Some(error);
    }

    #[cfg(feature = "internal_test")]
    pub fn reconnect_count(&self) -> u64 {
        // - 1 for initial value
        self.new_client_rx
            .borrow()
            .value_version()
            .saturating_sub(1)
    }
}

#[async_trait::async_trait(?Send)]
impl super::AsClient for Client {
    async fn send<R: protocol::api::Request>(&self, request: &R) -> Result<R::Response, Error> {
        self.handle_reconnect().await?;
        // This is an Rc clone so it is cheap.
        // It is used not to hold Ref across await point in send.
        let client = self.client.borrow().clone().expect("already set");

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
    use crate::network::AsClient as _;
    use crate::test::util::TARANTOOL_LISTEN;

    const _3_SEC: Duration = Duration::from_secs(3);

    fn test_client() -> Client {
        Client::with_config(
            "localhost".to_string(),
            TARANTOOL_LISTEN,
            protocol::Config {
                creds: Some(("test_user".into(), "password".into())),
            },
        )
    }

    #[crate::test(tarantool = "crate")]
    fn connect_failure() {
        fiber::block_on(async {
            // Can be any other unused port
            let client = Client::new("localhost".to_string(), 3300);
            let err = client.ping().await.unwrap_err();
            assert!(matches!(dbg!(err), Error::Tcp(_)))
        });
    }

    #[crate::test(tarantool = "crate")]
    fn ping_after_reconnect() {
        fiber::block_on(async {
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
        });
    }

    #[crate::test(tarantool = "crate")]
    fn reconnect_now_vs_later() {
        fiber::block_on(async {
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
        });
    }

    // More of an example of how this client can be used than a test
    #[crate::test(tarantool = "crate")]
    fn reconnect_on_network_error() {
        use std::io::{Error, ErrorKind};
        fiber::block_on(async {
            let client = test_client();

            client.inject_error(Error::from(ErrorKind::ConnectionAborted).into());
            client.ping().timeout(_3_SEC).await.unwrap_err();
            client.reconnect_now().await.unwrap();
            assert_eq!(client.reconnect_count(), 1);

            client.inject_error(Error::from(ErrorKind::ConnectionAborted).into());
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
                .unwrap()
                .decode::<(i32,)>()
                .unwrap();
            // value received on an old connection, though there was a reconnect request
            assert_eq!(result, (42,));
            // Globally the client has 1 reconnection
            assert_eq!(client.reconnect_count(), 1);
            // but this clone of a client does not yet use the new connection
            assert!(client.new_client_rx.borrow().has_changed());

            client.ping().timeout(_3_SEC).await.unwrap();
            // it does use the new connection for the new request
            assert!(!client.new_client_rx.borrow().has_changed());
        });
        jh.join();
    }

    #[crate::test(tarantool = "crate")]
    fn concurrent_messages_one_fiber() {
        fiber::block_on(async {
            let client = test_client();
            let mut ping_futures = vec![];
            for _ in 0..10 {
                ping_futures.push(client.ping());
            }
            for res in futures::future::join_all(ping_futures).await {
                res.unwrap();
            }
        });
    }
}
