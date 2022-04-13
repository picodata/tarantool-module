async fn foo() {
    //sleep 1
    // return "foo"
}

struct FooFut {
    state: Enum { ... }
}

impl Future for FooFut {
    fn poll(&self, waker: Waker) -> Poll {
        ...
    }
}

async fn bar() {
    // sleep 1
    // return "bar"
}

struct BarFut {
    state: Enum { ... }
}

impl Future for BarFut {
    fn poll(&self, waker: Waker) -> Poll {
        ...
    }
}

struct AnonFut {
    state: Enum { BeforeFoo, WaitingFoo, WaitingBar },
}

impl Future for AnonFut {
    fn poll(&self, waker: Waker) -> Poll {
        match self.state {
            BeforeFoo => {
                println!("hello");
                self.state = WaitingFoo(foo());
                return Poll::Pending
            }
            WaitingFoo(foo) => {
                match foo.poll(...) {
                    Poll::Ready(new_x) => {
                        x = new_x
                    },
                    Poll::Pending => {
                        self.state = WaitingFoo;
                        return Poll::Pending
                    }
                }
            }
        }
    }
}

fn main() {
    let executor = Executor::new();

    executor.spawn(async {
        println!("hello");
        let x: Future<T> = foo(); //.await;
        let x: T = foo().await;
        let y = bar().await;
        let z = select!(foo(), bar()).await;
        println!("{x}, {y}");
    })
}
