use futures_core::ready;
use pin_project::{pin_project, project};
use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use tower_service::Service;

/// A `Future` consuming a `Service` and request, waiting until the `Service`
/// is ready, and then calling `Service::call` with the request, and
/// waiting for that `Future`.
#[pin_project]
#[derive(Debug)]
pub struct Oneshot<S: Service<Req>, Req> {
    #[pin]
    state: State<S, Req>,
}

#[pin_project]
enum State<S: Service<Req>, Req> {
    NotReady(S, Option<Req>),
    Called(#[pin] S::Future),
    Done,
}

impl<S, Req> fmt::Debug for State<S, Req>
where
    S: Service<Req> + fmt::Debug,
    Req: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            State::NotReady(s, Some(req)) => f
                .debug_tuple("State::NotReady")
                .field(s)
                .field(req)
                .finish(),
            State::NotReady(_, None) => unreachable!(),
            State::Called(_) => f.debug_tuple("State::Called").field(&"S::Future").finish(),
            State::Done => f.debug_tuple("State::Done").finish(),
        }
    }
}

impl<S, Req> Oneshot<S, Req>
where
    S: Service<Req>,
{
    #[allow(missing_docs)]
    pub fn new(svc: S, req: Req) -> Self {
        Oneshot {
            state: State::NotReady(svc, Some(req)),
        }
    }
}

impl<S, Req> Future for Oneshot<S, Req>
where
    S: Service<Req>,
{
    type Output = Result<S::Response, S::Error>;

    #[project]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        loop {
            #[project]
            match this.state.as_mut().project() {
                State::NotReady(svc, req) => {
                    let _ = ready!(svc.poll_ready(cx))?;
                    let f = svc.call(req.take().expect("already called"));
                    this.state.set(State::Called(f));
                }
                State::Called(fut) => {
                    let res = ready!(fut.poll(cx))?;
                    this.state.set(State::Done);
                    return Poll::Ready(Ok(res));
                }
                State::Done => panic!("polled after complete"),
            }
        }
    }
}
