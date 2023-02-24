use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use twilight_http::{response::ResponseFuture, Error, Response};

pin_project_lite::pin_project! {
    /// A queue for futures that resolve to a [`Response`](Response).
    pub struct ResponseQueue<T> {
        #[pin]
        pending: VecDeque<ResponseFuture<T>>,
    }
}

impl<T> ResponseQueue<T> {
    /// Append a future to the queue.
    pub fn push(&mut self, future: ResponseFuture<T>) {
        self.pending.push_back(future);
    }
}

impl<T> Default for ResponseQueue<T> {
    fn default() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }
}

impl<T: Unpin> Future for ResponseQueue<T> {
    type Output = Result<Response<T>, Error>;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let pending = this.pending.get_mut();

        if let Some(front) = pending.front_mut() {
            let front = Pin::new(front);

            front.poll(context).map(|value| {
                pending.pop_front();

                value
            })
        } else {
            Poll::Pending
        }
    }
}
