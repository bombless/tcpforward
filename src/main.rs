use std::collections::HashMap;
use std::io;

use structopt::StructOpt;
use tokio::io::copy;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Eq, PartialEq, Hash)]
enum TaskType {
    WriteTask,
    ReadTask,
}

async fn process_conn(local: TcpStream, remote: TcpStream) {
    let (mut local_reader, mut local_writer) = local.into_split();
    let (mut remote_reader, mut remote_writer) = remote.into_split();

    let mut tasks_map: HashMap<TaskType, JoinHandle<_>> = HashMap::new();

    let write_task = tokio::spawn(async move {
        copy(&mut local_reader, &mut remote_writer).await
    });
    tasks_map.insert(TaskType::WriteTask, write_task);

    let read_task = tokio::spawn(async move {
        copy(&mut remote_reader, &mut local_writer).await
    });
    tasks_map.insert(TaskType::ReadTask, read_task);

    for (task_type, task) in tasks_map.iter_mut() {
        let result = task.await.unwrap();
        match result {
            Ok(n) => {
                match task_type {
                    TaskType::WriteTask => println!("write {:?} bytes to remote!", n),
                    TaskType::ReadTask => println!("read {:?} bytes from remote!", n),
                }
            }
            Err(e) => {
                println!("something went error: {:?}", e.to_string());
                match task_type {
                    TaskType::WriteTask => tasks_map.get(&TaskType::ReadTask).unwrap().abort(),
                    TaskType::ReadTask => tasks_map.get(&TaskType::WriteTask).unwrap().abort(),
                }
                break;
            }
        }
    }

    // Bellow codes will cause a high cpu usage!!!
    //
    // let wb1 = Arc::new(Mutex::new(Vec::<u8>::new()));
    // let wb2 = Arc::clone(&wb1);
    // let rb1 = Arc::new(Mutex::new(Vec::<u8>::new()));
    // let rb2 = Arc::clone(&rb1);
    //
    // let mut tasks = Vec::new();
    //
    // let local_task = tokio::spawn(async move {
    //     loop {
    //         let ready = local.ready(Interest::READABLE | Interest::WRITABLE).await.unwrap();
    //
    //         if ready.is_readable() {
    //             debug!("local is readable");
    //
    //             let mut buf = [0; 1024];
    //             match local.try_read(&mut buf) {
    //                 Ok(0) => {
    //                     info!("local stream's read half is closed!");
    //                     return;
    //                 }
    //                 Ok(n) => {
    //                     debug!("read from local: {:?}", &buf[..n]);
    //                     let mut buf_ = wb1.lock().unwrap();
    //                     buf_.extend(&buf[..n]);
    //                 }
    //                 Err(ref e)  if e.kind() == io::ErrorKind::WouldBlock => {
    //                     continue;
    //                 }
    //                 Err(e) => {
    //                     println!("{:?}", e.to_string());
    //                     return;
    //                 }
    //             }
    //         }
    //
    //         if ready.is_writable() {
    //             debug!("local is writable");
    //
    //             let mut buf = rb2.lock().unwrap();
    //             if buf.len() == 0 {
    //                 continue;
    //             }
    //
    //             debug!("begin to write to local: {:?}", buf);
    //
    //             match local.try_write(&buf) {
    //                 Ok(n) => {
    //                     if n == buf.len() {
    //                         *buf = Vec::<u8>::new();
    //                         continue;
    //                     }
    //                     *buf = Vec::from(&buf[n..]);
    //                 }
    //                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    //                     continue;
    //                 }
    //                 Err(e) => {
    //                     error!("{:?}", e.to_string());
    //                     return;
    //                 }
    //             }
    //         }
    //     }
    // });
    // tasks.push(local_task);
    //
    // let remote_task = tokio::spawn(async move {
    //     let remote = TcpStream::connect(
    //         format!("{}:{}", remote_ip, remote_port)
    //     ).await.unwrap();
    //
    //     loop {
    //         let ready = remote.ready(Interest::READABLE | Interest::WRITABLE).await.unwrap();
    //
    //         if ready.is_readable() {
    //             debug!("remote is readable");
    //
    //             let mut buf = [0; 1024];
    //             match remote.try_read(&mut buf) {
    //                 Ok(0) => {
    //                     info!("remote stream's read half is closed!");
    //                     return;
    //                 }
    //                 Ok(n) => {
    //                     debug!("read from remote: {:?}", &buf[..n]);
    //                     let mut buf_ = rb1.lock().unwrap();
    //                     buf_.extend(&buf[..n]);
    //                 }
    //                 Err(ref e)  if e.kind() == io::ErrorKind::WouldBlock => {
    //                     continue;
    //                 }
    //                 Err(e) => {
    //                     error!("{:?}", e.to_string());
    //                     return;
    //                 }
    //             }
    //         }
    //
    //         if ready.is_writable() {
    //             debug!("remote is writable");
    //
    //             let mut buf = wb2.lock().unwrap();
    //             if buf.len() == 0 {
    //                 continue;
    //             }
    //
    //             debug!("begin to write to remote: {:?}", buf);
    //
    //             match remote.try_write(&buf) {
    //                 Ok(n) => {
    //                     if n == buf.len() {
    //                         *buf = Vec::<u8>::new();
    //                         continue;
    //                     }
    //                     *buf = Vec::from(&buf[n..]);
    //                 }
    //                 Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
    //                     continue;
    //                 }
    //                 Err(e) => {
    //                     error!("{:?}", e.to_string());
    //                     return;
    //                 }
    //             }
    //         }
    //     }
    // });
    // tasks.push(remote_task);
    //
    // for task in tasks {
    //     task.await.unwrap();
    // }
}

/// A simple tcp forwarding tool
#[derive(StructOpt, Debug)]
#[structopt(name = "tcpforward")]
struct Options {
    /// local ip
    #[structopt(long)]
    local_ip: String,

    /// local port
    #[structopt(long)]
    local_port: u16,

    /// remote ip
    #[structopt(long)]
    remote_ip: String,

    /// remote port
    #[structopt(long)]
    remote_port: u16,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    println!("service is starting ...");

    let options: Options = Options::from_args();
    let listener = TcpListener::bind(
        format!("{}:{}", options.local_ip, options.local_port)
    ).await?;

    loop {
        let (local, peer_addr) = listener.accept().await?;
        println!("a new connection {:?} is coming!", peer_addr);

        let remote = match TcpStream::connect(
            format!("{}:{}", options.remote_ip, options.remote_port)
        ).await {
            Ok(s) => s,
            Err(e) => {
                println!("connect to remote error: {:?}", e.to_string());
                continue;
            }
        };
        tokio::spawn(async move {
            process_conn(local, remote).await;
        });
    }
}