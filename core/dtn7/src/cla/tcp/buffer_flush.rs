use core::pin::Pin;
use futures::future::FusedFuture;
use futures::stream::Fuse;
use futures::{Future, StreamExt, TryStream};
use futures_util::ready;
use futures_util::stream::Stream;
use futures_util::task::{Context, Poll};
use futures_util::Sink;
use pin_project_lite::pin_project;

pin_project! {
    /// Future for the [`forward`](super::StreamExt::forward) method.
    #[project = ForwardProj]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct ForwardFlush<St, Si, Item> {
        #[pin]
        sink: Option<Si>,
        #[pin]
        stream: Fuse<St>,
        buffered_item: Option<Item>,
        capacity: usize,
        last_flush: usize,
    }
}

impl<St, Si, Item> ForwardFlush<St, Si, Item>
where
    St: Stream,
{
    pub(crate) fn new(stream: St, sink: Si, capacity: usize) -> Self {
        Self {
            sink: Some(sink),
            stream: stream.fuse(),
            buffered_item: None,
            capacity,
            last_flush: 0,
        }
    }
}

impl<St, Si, Item, E> FusedFuture for ForwardFlush<St, Si, Item>
where
    Si: Sink<Item, Error = E>,
    St: Stream<Item = Result<Item, E>>,
{
    fn is_terminated(&self) -> bool {
        self.sink.is_none()
    }
}

impl<St, Si, Item, E> Future for ForwardFlush<St, Si, Item>
where
    Si: Sink<Item, Error = E>,
    St: Stream<Item = Result<Item, E>>,
{
    type Output = Result<(), E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let ForwardProj {
            mut sink,
            mut stream,
            buffered_item,
            capacity,
            last_flush,
        } = self.project();
        let mut si = sink
            .as_mut()
            .as_pin_mut()
            .expect("polled `Forward` after completion");

        loop {
            // If we've got an item buffered already, we need to write it to the
            // sink before we can do anything else
            if buffered_item.is_some() {
                ready!(si.as_mut().poll_ready(cx))?;
                si.as_mut().start_send(buffered_item.take().unwrap())?;
                // increment last_flush for each item send to sink
                *last_flush += 1;
            }

            // if enough items are in sink, flush and only accept new items afterwards
            if last_flush >= capacity {
                ready!(si.as_mut().poll_flush(cx))?;
                *last_flush = 0;
            }

            match stream.as_mut().poll_next(cx)? {
                Poll::Ready(Some(item)) => {
                    *buffered_item = Some(item);
                }
                Poll::Ready(None) => {
                    ready!(si.poll_close(cx))?;
                    sink.set(None);
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => {
                    ready!(si.poll_flush(cx))?;
                    *last_flush = 0;
                    return Poll::Pending;
                }
            }
        }
    }
}

pub(crate) trait StreamCustomExt: Stream {
    fn forward_flush<S>(self, sink: S, capacity: usize) -> ForwardFlush<Self, S, Self::Ok>
    where
        S: Sink<Self::Ok, Error = Self::Error>,
        Self: TryStream + Sized,
    {
        ForwardFlush::new(self, sink, capacity)
    }
}

impl<T: ?Sized> StreamCustomExt for T where T: Stream {}
