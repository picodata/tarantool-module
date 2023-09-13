use hyper::{service::Service, Uri};
use std::{
    future::Future,
    net::SocketAddr,
    pin::Pin,
    task::{self, Poll},
};
use tarantool::fiber;
use tarantool::network::client::tcp::TcpStream;
use tarantool::proc;

#[derive(Clone)]
struct Connector;

impl Service<Uri> for Connector {
    type Response = TcpStream;
    type Error = tarantool::network::client::tcp::Error;
    // We can't "name" an `async` generated future.
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _: &mut task::Context<'_>) -> Poll<Result<(), Self::Error>> {
        // This connector is always ready, but others might not be.
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        Box::pin(async move {
            let tcp = TcpStream::connect(dbg!(uri.host().unwrap()), 80).await;
            dbg!(tcp)
        })
    }
}

#[proc]
fn get(url: String) {
    let client = hyper::Client::builder()
        .executor(fiber::r#async::FiberExecutor)
        .build::<_, hyper::Body>(Connector);
    fiber::block_on(async move {
        let res = client.get(url.parse().unwrap()).await.unwrap();

        // And then, if the request gets a response...
        println!("status: {}", res.status());

        // Concatenate the body stream into a single buffer...
        let buf = hyper::body::to_bytes(res).await.unwrap();

        println!("body: {:?}", buf);
    });
}
