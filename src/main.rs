use std::collections::HashMap;
use std::io;

use std::net::SocketAddr;
use std::sync::Arc;

use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

#[derive(Eq, PartialEq, Hash)]
enum TaskType {
    WriteTask,
    ReadTask,
}

#[derive(Clone)]
struct Client {
    addr: SocketAddr,
    local_port: u16,
    pos: usize,
    blocking: Option<bool>,
    search: Arc<Vec<String>>,
    pattern_or: bool,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} as port :{} {}", self.addr, self.local_port, if self.blocking == Some(true) { "(blocking)" } else { "" })
    }
}

mod copy;

async fn process_conn(local: TcpStream, remote: TcpStream, mut client: Client, password: Option<Arc<String>>) {
    let (mut local_reader, mut local_writer) = local.into_split();
    let (mut remote_reader, mut remote_writer) = remote.into_split();

    let mut tasks_map: HashMap<TaskType, JoinHandle<_>> = HashMap::new();

    let addr = client.addr;

    let login_mode = password.is_some();

    if login_mode {
        let mut client_writer = client.clone();
        client_writer.blocking = None;
        let write_task = tokio::spawn(async move {
            copy::copy(&mut local_reader, &mut remote_writer, &mut client_writer, None).await
        });
        tasks_map.insert(TaskType::WriteTask, write_task);

        
        let read_task = tokio::spawn(async move {
            copy::copy(&mut remote_reader, &mut local_writer, &mut client, password).await
        });
        tasks_map.insert(TaskType::ReadTask, read_task);
    } else {
        let write_task = tokio::spawn(async move {
            copy::copy(&mut local_reader, &mut remote_writer, &mut client, None).await
        });
        tasks_map.insert(TaskType::WriteTask, write_task);
    
        let read_task = tokio::spawn(async move {
            tokio::io::copy(&mut remote_reader, &mut local_writer).await
        });
        tasks_map.insert(TaskType::ReadTask, read_task);

    }

    for (task_type, task) in tasks_map.iter_mut() {
        let result = task.await.unwrap();
        match result {
            Ok(n) => {
                let date = chrono::Local::now();
                match task_type {
                    TaskType::WriteTask => println!("[{}] write {:?} bytes to remote {:?}!", date.format("%m-%d %H:%M"), n, addr),
                    TaskType::ReadTask => println!("[{}] read {:?} bytes from remote {:?}!", date.format("%m-%d %H:%M"), n, addr),
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

    /// search
    #[structopt(long)]
    search: Vec<String>,

    /// blocking mode
    #[structopt(long)]
    blocking_mode: bool,

    /// pattern or
    #[structopt(long)]
    pattern_or: bool,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut options: Options = Options::from_args();
    let password = std::mem::replace(&mut options.password, None).map(Arc::new);
    println!("search pattern {} {:?}", if options.pattern_or { "or" } else { "and" }, options.search);
    let search = Arc::new(options.search.clone());
    let pattern_or = options.pattern_or;
    let date = chrono::Local::now();
    println!("[{}] service is starting ...", date.format("%m-%d %H:%M"));

    let listener = TcpListener::bind(
        format!("{}:{}", options.local_ip, options.local_port)
    ).await?;


    loop {
        let (local, peer_addr) = listener.accept().await?;
        let date = chrono::Local::now();
        println!("[{}] a new connection {:?} is coming!", date.format("%m-%d %H:%M"), peer_addr);

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
        let search = search.clone();
        let blocking_mode = if options.blocking_mode { Some(false) } else { None };
        tokio::spawn(async move {
            let local_port = remote.local_addr().unwrap().port();
            process_conn(local, remote, Client { addr: peer_addr, pos: 0, blocking: blocking_mode, local_port, search, pattern_or }, password).await;
        });
    }
}
