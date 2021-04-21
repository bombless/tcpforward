use std::collections::VecDeque;
use std::io;
use std::sync::{Arc, Mutex};

use tokio::io::Interest;
use tokio::net::{TcpListener, TcpStream};

async fn process_conn(local: TcpStream) {
    let wb1 = Arc::new(Mutex::new(VecDeque::<u8>::new()));
    let wb2 = Arc::clone(&wb1);
    let rb1 = Arc::new(Mutex::new(VecDeque::<u8>::new()));
    let rb2 = Arc::clone(&rb1);

    let remote = TcpStream::connect("127.0.0.1:1234").await.unwrap();

    tokio::spawn(async move {
        loop {
            let ready = local.ready(Interest::READABLE | Interest::WRITABLE).await.unwrap();
            // println!("local is ready!");

            if ready.is_readable() {
                // println!("local is readable");

                let mut buf = [0; 1024];
                match local.try_read(&mut buf) {
                    Ok(0) => {
                        println!("local stream's read half is closed!");
                        return;
                    }
                    Ok(n) => {
                        println!("read from local: {:?}", &buf[..n]);
                        let mut queue = wb1.lock().unwrap();
                        queue.extend(&buf[..n]);
                    }
                    Err(ref e)  if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        println!("{:?}", e.to_string());
                        return;
                    }
                }
            }

            if ready.is_writable() {
                // println!("local is writable");

                let mut queue = rb2.lock().unwrap();
                if queue.len() == 0 {
                    continue;
                }

                let buf = queue.make_contiguous();
                println!("begin to write to local: {:?}", buf);

                match local.try_write(buf) {
                    Ok(n) => {
                        if n == buf.len() {
                            *queue = VecDeque::<u8>::new();
                            continue;
                        }
                        *queue = VecDeque::from(buf[n..].to_vec());
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        println!("{:?}", e.to_string());
                        return;
                    }
                }
            }
        }
    });

    tokio::spawn(async move {
        loop {
            let ready = remote.ready(Interest::READABLE | Interest::WRITABLE).await.unwrap();
            // println!("remote is ready!");

            if ready.is_readable() {
                // println!("remote is readable");

                let mut buf = [0; 1024];
                match remote.try_read(&mut buf) {
                    Ok(0) => {
                        println!("remote stream's read half is closed!");
                        return;
                    }
                    Ok(n) => {
                        println!("read from remote: {:?}", &buf[..n]);
                        let mut queue = rb1.lock().unwrap();
                        queue.extend(&buf[..n]);
                    }
                    Err(ref e)  if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        println!("{:?}", e.to_string());
                        return;
                    }
                }
            }

            if ready.is_writable() {
                // println!("remote is writable");

                let mut queue = wb2.lock().unwrap();
                if queue.len() == 0 {
                    continue;
                }

                let buf = queue.make_contiguous();
                println!("begin to write to remote: {:?}", buf);

                match remote.try_write(buf) {
                    Ok(n) => {
                        if n == buf.len() {
                            *queue = VecDeque::<u8>::new();
                            continue;
                        }
                        *queue = VecDeque::from(buf[n..].to_vec());
                    }
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        println!("{:?}", e.to_string());
                        return;
                    }
                }
            }
        }
    });
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:12345").await?;

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("a new connection {:?} is coming!", addr);
        process_conn(socket).await;
    }
}