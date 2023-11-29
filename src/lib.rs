pub mod ivyerror;
// mod peer;
mod ivy_messages;

// use std::{net::{TcpListener, SocketAddr, TcpStream}, fmt::write, str::FromStr, io::Read, thread::{self, JoinHandle}, sync::{Arc, Mutex}};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use socket2::{Socket, Domain, Type, Protocol};
use ivyerror::IvyError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket, tcp::OwnedReadHalf, tcp::OwnedWriteHalf};
use tokio::sync::{futures, Mutex};
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
    bus: Option<Arc<BusPrivate>>,
    //port: Option<u16>,
    token: CancellationToken,
    
    //handles: Arc<Mutex<Vec<JoinHandle<()>>>>,

    //bus: Option<BusPrivate>
}

struct BusPrivate {
    domain: String,
    handles: Mutex<Vec<JoinHandle<()>>>,
    port: u16
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
        IvyBus {
            appname: appname.to_string(),
            bus: None,
            //port: None,
            token: CancellationToken::new(),
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
        //self.port = Some(port);
        println!("listen on TCP {port}");

        self.bus = Some(Arc::new(BusPrivate {
            domain: domain.into(),
            handles: Mutex::new(vec![]),
            port
        }));


        let token_udp = self.token.clone();

        self.start_udp_watcher(udp_arc, address, token_udp).await?;


        let tok = self.token.clone();

        let bus = self.bus.clone().unwrap();

        let tcp_listener_handle = tokio::spawn(async move {
            loop {
                println!("wait for new TCP connection");
                
                tokio::select! {
                    _ = tok.cancelled() => {
                        println!("wait for TCP connection canceled");
                        break;
                    }
                    Ok((socket, _)) = listener.accept() => {
                        let (mut read_half, write_half) = socket.into_split();
                        
                        println!("accepting TCP connection!");
                        let cloned_token = tok.clone();
                        let h = tokio::spawn(async move {
                            tokio::select! {
                                // Step 3: Using cloned token to listen to cancellation requests
                                _ = cloned_token.cancelled() => {
                                    println!("task canceled");
                                    //println!("task canceled {}", read_half.peer_addr().unwrap().port());
                                }
                                _ = IvyBus::tcp_read(&mut read_half) => {
                                    // Long work has completed
                                }
                            }
                        });
                        bus.handles.lock().await.push(h);
                    }                    
                }
            }


        });

        self.bus.clone().unwrap().handles.lock().await.push(tcp_listener_handle);

        println!("this is the end");

        Ok(())
    }


    async fn tcp_read(read_half: &mut OwnedReadHalf) {
        let mut buf = vec![0; 1024];
    
        // In a loop, read data from the socket and write the data back.
        loop {
            let n = read_half
                .read(&mut buf)
                .await
                .expect("failed to read data from socket");
            
                let lines = buf[0..n]
                    .split(|c| *c==b'\n')
                    .filter(|buf| buf.len() > 0);
                for line in lines {
                    let msg = IvyMsg::parse(line);
                    //let s = String::from_utf8(line.to_vec()).unwrap();
                    //println!("rcv: {s}");
                    println!("{msg:?}");
                }
        }
    }

    async fn start_udp_watcher(&mut self, udp_arc: Arc<UdpSocket>, address: SocketAddr, token_udp: CancellationToken) -> Result<(), IvyError> {
        let port = self.bus.clone().ok_or(IvyError::BadDomain)?.port;
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

            let tok = token_udp.clone();
            loop {
                let mut buf = vec![0; 1024];
                println!("udp listen...");
                tokio::select! {
                    _ = tok.cancelled() => {
                        println!("UDP task canceled");
                        break;
                    }
                    Ok((n, src)) = udp_arc.recv_from(&mut buf) => {
                        println!("received {n} bytes from {src}:");
                        println!("{}", String::from_utf8(buf[0..n].to_owned()).unwrap());
                    }                    
                }
            }
            println!("udp ended");
        });
        self.bus.clone().unwrap().handles.lock().await.push(handle);
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.token.cancel();
        for handle in self.bus.clone().unwrap().handles.lock().await.iter_mut() {
            let _ = handle.await;
        }
    }

}

