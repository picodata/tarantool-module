pub fn timer() {
    let f = async {
        Timer::new(100.millis()).await;
        "done"
    };
    let mut f = Box::pin(f);

    let task = Task(Cell::new(false));
    let (task, task_rx) = Rc::new(task).into_clones();
    let waker = task.into_waker();

    assert_eq!(f.as_mut().poll(&mut Context::from_waker(&waker)), Poll::Pending);
    assert_eq!(task_rx.0.get(), false);

    let res = loop {
        match f.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Pending => sleep(100.millis()),
            Poll::Ready(res) => break res,
        }
    };
    assert_eq!(res, "done");
    assert_eq!(task_rx.0.get(), true);
}

struct Task<T>(T);

impl RcWake for Task<Cell<bool>> {
    fn wake_by_ref(self: &Rc<Self>) {
        (**self).0.set(true)
    }
}

use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    task::{Context, Poll},
};

use tarantool::{
    fiber::{
        future::{RcWake, Timer},
        sleep,
    },
    util::{IntoClones, ToDuration},
};
