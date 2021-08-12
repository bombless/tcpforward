use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use std::future::Future;
use std::io;
use kmp::kmp_find;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::sync::Arc;


macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

#[derive(Debug)]
pub(super) struct CopyBuffer<'a> {
    read_done: bool,
    pos: usize,
    cap: usize,
    amt: u64,
    buf: Vec<u8>,
    client: &'a mut crate::Client,
    password_segment: Option<String>,
}


fn replace(from: &[u8], to: &[u8], content: &mut Vec<u8>, length: usize) {
    let search: Option<usize> = kmp_find(from, &content);
    if let Some(idx) = search {
        if idx + from.len() >= length || idx + to.len() >= length {
            return
        }
        let mut new_vec = Vec::from(&content[0 .. idx]);
        new_vec.extend(to);
        new_vec.extend(&content[idx + from.len() .. ]);
        *content = new_vec;
    }
}

fn modify_buffer(buffer: &mut Vec<u8>, length: usize, password_segment: &str) -> isize {
    let replaces: [[&[u8]; 2]; 2] = [
                    [b"{this._beforeLogin()}", b"{this._beforeLogin();this._onLogin()}"],
                    [b"s=o.getValue(),r=n.getValue()", password_segment.as_bytes()],
                ];

    let old_length = buffer.len();

    let old_buffer = buffer.clone();

    for [from, to] in &replaces {
        replace(from, to, buffer, length);
    }

    let new_length = buffer.len();

    if new_length != old_length {
        replace(b"CONTENT-LENGTH: 6236", format!("CONTENT-LENGTH: {}", 6236 + new_length - old_length).as_bytes(), buffer, length + new_length - old_length);
        println!("come on {} {}", std::str::from_utf8(&old_buffer).unwrap(), std::str::from_utf8(buffer).unwrap());
    }

    return new_length as isize - old_length as isize;
}

fn log(data: &[u8], client: &crate::Client) {
    eprint!("{:?}({}..{}):", client, client.pos, client.pos + data.len());
    for b in data {
        eprint!("{:02x}", b)
    }
    eprintln!()
}

impl<'a> CopyBuffer<'a> {
    pub(super) fn new(client: &'a mut crate::Client, password: Option<&str>) -> Self {
        Self {
            read_done: false,
            pos: 0,
            cap: 0,
            amt: 0,
            // buf: vec![0; 65536].into_boxed_slice(),
            buf: vec![0; 65536],
            client,
            password_segment: password.map(|x| format!("s='admin',r='{}'", x)),
        }
    }

    pub(super) fn poll_copy<R, W>(
        &mut self,
        cx: &mut Context<'_>,
        mut reader: Pin<&mut R>,
        mut writer: Pin<&mut W>,
    ) -> Poll<io::Result<u64>>
    where
        R: AsyncRead + ?Sized,
        W: AsyncWrite + ?Sized,
    {
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if self.pos == self.cap && !self.read_done {
                let me = &mut *self;
                let mut buf = ReadBuf::new(&mut me.buf);
                ready!(reader.as_mut().poll_read(cx, &mut buf))?;
                let n = buf.filled().len();
                if n == 0 {
                    self.read_done = true;
                } else {
                    self.pos = 0;
                    if let Some(password_segment) = &self.password_segment {
                        self.cap = (modify_buffer(&mut self.buf, n, password_segment) + n as isize) as usize;
                    } else {
                        log(&self.buf[..n], &self.client)
                    }
                    self.client.pos += n
                }
            }

            // If our buffer has some data, let's write it out!
            while self.pos < self.cap {
                let me = &mut *self;

                let i = ready!(writer.as_mut().poll_write(cx, &me.buf[me.pos..me.cap]))?;
                if i == 0 {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "write zero byte into writer",
                    )));
                } else {
                    // self.pos += i + old_length - new_length;
                    self.pos += i;
                    self.amt += i as u64;
                }
            }

            // If we've written all the data and we've seen EOF, flush out the
            // data and finish the transfer.
            if self.pos == self.cap && self.read_done {
                ready!(writer.as_mut().poll_flush(cx))?;
                return Poll::Ready(Ok(self.amt));
            }
        }
    }
}

/// A future that asynchronously copies the entire contents of a reader into a
/// writer.
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
struct Copy<'a, R: ?Sized, W: ?Sized> {
    reader: &'a mut R,
    writer: &'a mut W,
    buf: CopyBuffer<'a>,
}
pub(super) async fn copy<'a, R, W>(reader: &'a mut R, writer: &'a mut W, client: &mut crate::Client, password: Option<Arc<String>>) -> io::Result<u64>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    Copy {
        reader,
        writer,
        buf: CopyBuffer::new(client, match password { Some(ref x) => Some(&x), None => None }),
    }.await
}

impl<R, W> Future for Copy<'_, R, W>
where
    R: AsyncRead + Unpin + ?Sized,
    W: AsyncWrite + Unpin + ?Sized,
{
    type Output = io::Result<u64>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let me = &mut *self;

        me.buf
            .poll_copy(cx, Pin::new(&mut *me.reader), Pin::new(&mut *me.writer))
    }
}