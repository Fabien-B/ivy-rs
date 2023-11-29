pub mod ivyerror;
// mod peer;
mod ivy_messages;

// use std::{net::{TcpListener, SocketAddr, TcpStream}, fmt::write, str::FromStr, io::Read, thread::{self, JoinHandle}, sync::{Arc, Mutex}};
use std::net::{SocketAddr};
use std::sync::{Arc};
use std::time::Duration;
use socket2::{Socket, Domain, Type, Protocol};
use ivyerror::IvyError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket, tcp::OwnedReadHalf, tcp::OwnedWriteHalf, TcpStream};
use tokio::sync::{futures, Mutex, RwLock, mpsc};
use tokio::time::sleep;
use tokio::{select, join};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

//use tokio::time::{sleep, Duration};

use crate::ivy_messages::IvyMsg;
// use peer::Peer;
// use std::time::Duration;

const PROTOCOL_VERSION: u32 = 3;



pub struct IvyBus {
    pub appname: String,
    bus: Arc<BusPrivate>,
}

struct BusPrivate {
    domain: RwLock<String>,
    handles: Mutex<Vec<JoinHandle<()>>>,
    port: RwLock<Option<u16>>,
    token: CancellationToken,
    //subscriptions: Vec<(String, Fn)>,
    //udp_handle: JoinHandle<()>,
    //tcp_handle: JoinHandle<()>,
    ////tcp_listener: TcpListener,
    //listener: std::thread::JoinHandle<_>,
    //watcher_id: String,
    //peers: Arc<Mutex<Vec<Peer>>>,
}

impl IvyBus {

    pub fn new(appname: &str) -> Self {


        let bp = BusPrivate {
            domain: RwLock::new("".into()),
            handles: Mutex::new(vec![]),
            port: RwLock::new(None),
            token: CancellationToken::new()
        };

        IvyBus {
            appname: appname.to_string(),
            bus: Arc::new(bp),
            //handles: Arc::new(Mutex::new(vec![])),
        }
    }

    pub async fn start(&mut self, domain: &str) -> Result<(), IvyError> {
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        udp_socket.set_reuse_address(true)?;
        udp_socket.set_broadcast(true)?;
        udp_socket.set_nonblocking(true)?;
        udp_socket.bind(&address.into())?;
        let udp_socket = UdpSocket::from_std(udp_socket.into())?;
        let udp_arc = Arc::new(udp_socket);


        //let listener = TcpListener::bind("0.0.0.0:0").await?;
        let listener = TcpListener::bind("0.0.0.0:8888").await?;
        let port = listener.local_addr()?.port();
        *self.bus.port.write().await = Some(port);
        *self.bus.domain.write().await = domain.into();
        println!("listen on TCP {port}");


        self.start_udp_watcher(udp_arc, address, self.bus.clone()).await?;

        let bus = self.bus.clone();

        let tcp_listener_handle = tokio::spawn(async move {
            
            loop {
                println!("wait for new TCP connection");
                
                tokio::select! {
                    _ = bus.token.cancelled() => {
                        println!("wait for TCP connection canceled");
                        break;
                    }
                    Ok((socket, _)) = listener.accept() => {
                        println!("accepting TCP connection!");
                        let (tx, rx) = mpsc::channel::<u32>(2);
                        let bp = bus.clone();
                        let h = tokio::spawn(async move {
                            IvyBus::tcp_read(socket, rx, bp).await;
                        });
                        bus.handles.lock().await.push(h);
                    }                    
                }
            }


        });

        self.bus.clone().handles.lock().await.push(tcp_listener_handle);

        println!("this is the end");

        Ok(())
    }


    async fn tcp_read(mut socket: TcpStream, mut rx: mpsc::Receiver<u32>, bp: Arc<BusPrivate>) {
        let mut buf = vec![0; 1024];
        let tok = bp.token.clone();
        loop {
            tokio::select! {
                Ok(n) = socket.read(&mut buf) => {
                    let lines = buf[0..n]
                    .split(|c| *c==b'\n')
                    .filter(|buf| buf.len() > 0);
                    for line in lines {
                        let msg = IvyMsg::parse(line);
                        println!("{msg:?}");
                    }
                }
                _ = rx.recv() => {

                }
                _ = tok.cancelled() => {
                    println!("task canceled");
                    //println!("task canceled {}", read_half.peer_addr().unwrap().port());
                }
            }
        }
    }

    async fn start_udp_watcher(&mut self, udp_arc: Arc<UdpSocket>, address: SocketAddr, bp: Arc<BusPrivate>) -> Result<(), IvyError> {
        let port = self.bus.clone().port.read().await.ok_or(IvyError::BadInit)?;
        //let port = self.port.ok_or(IvyError::BadDomain)?;
        // watcherId to be sure no to treat our own announcement message.
        let watcher_id = format!("{}_{}", self.appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{PROTOCOL_VERSION} {port} {watcher_id} {}\n", self.appname);

        let handle = tokio::spawn(async move {

            //sleep(Duration::from_millis(100)).await;
            // send annoucement
            match udp_arc.send_to(announce.as_bytes(), address).await {
                Err(err) => print!("error: {err:?}"),
                Ok(size) => print!("{size} bytes transmitted!")
            }

            loop {
                let mut buf = vec![0; 1024];
                println!("udp listen...");
                tokio::select! {
                    _ = bp.token.cancelled() => {
                        println!("UDP task canceled");
                        break;
                    }
                    Ok((n, src)) = udp_arc.recv_from(&mut buf) => {
                        let rcv_str = String::from_utf8(buf[0..n].to_owned()).unwrap();
                        if announce == rcv_str {
                            println!("received back sended annouce");
                        } else {
                            println!("UDP rcv: {rcv_str}");
                        }
                    }                    
                }
            }
            println!("udp ended");
        });
        self.bus.clone().handles.lock().await.push(handle);
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.bus.token.cancel();
        for handle in self.bus.clone().handles.lock().await.iter_mut() {
            //handle.abort();
            let _ = handle.await;
        }
    }

}

