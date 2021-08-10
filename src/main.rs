use std::collections::HashMap;
use std::io;

use std::sync::Arc;

use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Eq, PartialEq, Hash)]
enum TaskType {
    WriteTask,
    ReadTask,
}

mod copy;

async fn process_conn(local: TcpStream, remote: TcpStream, password: Option<Arc<String>>) {
    let (mut local_reader, mut local_writer) = local.into_split();
    let (mut remote_reader, mut remote_writer) = remote.into_split();

    let mut tasks_map: HashMap<TaskType, JoinHandle<_>> = HashMap::new();

    let write_task = tokio::spawn(async move {
        tokio::io::copy(&mut local_reader, &mut remote_writer).await
    });
    tasks_map.insert(TaskType::WriteTask, write_task);

    let read_task = tokio::spawn(async move {
        match password {
            Some(password) => copy::copy(&mut remote_reader, &mut local_writer, password).await,
            None => tokio::io::copy(&mut remote_reader, &mut local_writer).await,
        }
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

    /// password
    #[structopt(long)]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut options: Options = Options::from_args();
    let password = std::mem::replace(&mut options.password, None).map(Arc::new);
    println!("service is starting ...");

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
        let password = password.clone();
        tokio::spawn(async move {
            process_conn(local, remote, password).await;
        });
    }
}
