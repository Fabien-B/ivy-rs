pub mod ivyerror;
// mod peer;
// mod ivy_messages;

// use std::{net::{TcpListener, SocketAddr, TcpStream}, fmt::write, str::FromStr, io::Read, thread::{self, JoinHandle}, sync::{Arc, Mutex}};
use std::net::{SocketAddr};
use std::sync::Arc;
use socket2::{Socket, Domain, Type, Protocol, SockAddr};
use ivyerror::IvyError;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::time::{sleep, Duration};
// use peer::Peer;
// use std::time::Duration;

const PROTOCOL_VERSION: u32 = 3;



pub struct IvyBus {
    pub appname: String,
    bus: Option<i32>
    //bus: Option<BusPrivate>
}

// struct BusPrivate {
//     domain: String,
//     //subscriptions: Vec<(String, Fn)>,
//     udp_handle: JoinHandle<()>,
//     tcp_handle: JoinHandle<()>,
//     ////tcp_listener: TcpListener,
//     //listener: std::thread::JoinHandle<_>,
//     watcher_id: String,
//     peers: Arc<Mutex<Vec<Peer>>>,
// }

impl IvyBus {

    pub fn new(appname: &str) -> Self {
        IvyBus {appname: appname.to_string(), bus: None}
    }

    pub async fn start(&mut self, domain: &str) -> Result<(), IvyError> {
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        udp_socket.set_reuse_address(true)?;
        udp_socket.set_broadcast(true)?;
        udp_socket.bind(&address.into())?;
        let udp_socket = UdpSocket::from_std(udp_socket.into())?;
        let udp_arc = Arc::new(udp_socket);


        //let listener = TcpListener::bind("0.0.0.0:0").await?;
        let listener = TcpListener::bind("0.0.0.0:8888").await?;
        let port = listener.local_addr()?.port();
        println!("listen on TCP {port}");


        // watcherId to be sure no to treat our own announcement message.
        let watcher_id = format!("{}_{}", self.appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{PROTOCOL_VERSION} {port} {watcher_id} {}\n", self.appname);

        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            // send annoucement
            match udp_arc.send_to(announce.as_bytes(), address).await {
                Err(err) => print!("error: {err:?}"),
                Ok(size) => print!("{size} bytes transmitted!")
            }

            loop {
                let mut buf = [0 as u8; 50];
                match udp_arc.recv_from(&mut buf).await {
                    Err(err) => {},
                    Ok((n, src)) => {
                        println!("received {n} bytes from {src}:");
                        println!("{}", String::from_utf8(buf[0..n].to_owned()).unwrap());
                    },
                }
            }
        });

        // udp_arc.send_to(announce.as_bytes(), address).await?;


        loop {
            let (mut socket, _) = listener.accept().await?;
            tokio::spawn(async move {
                let mut buf = vec![0; 1024];
    
                // In a loop, read data from the socket and write the data back.
                loop {
                    let n = socket
                        .read(&mut buf)
                        .await
                        .expect("failed to read data from socket");
    
                    if n == 0 {
                        return;
                    }
    
                    socket
                        .write_all(&buf[0..n])
                        .await
                        .expect("failed to write data to socket");
                    let s = String::from_utf8(buf[0..n].to_vec()).unwrap();
                    println!("rcv: {s}");
                }
            });
        }











        Ok(())
    }

//         let tcp_listener: TcpListener = socket.into();




//         let peers = Arc::new(Mutex::new(Vec::new()));

//         let tcpeers = peers.clone();

//         let tcp_handle = std::thread::spawn(move || {
//             // TODO mpsc channel to stop it
//             for stream in tcp_listener.incoming() {
//                 match stream {
//                     Ok(stream) => {
//                         let p = Peer::handle_incoming(stream);
//                         tcpeers.lock().unwrap().push(p);
//                     }
//                     Err(e) => { println!("fail: {:?}", e);}
//                 }
        
//             }
//         });

//         self.bus = BusPrivate{domain: domain.to_string(), udp_handle, tcp_handle, watcher_id, peers}.into();
        
//         Ok(())
//     }

//     pub fn join(self) {
//         match self.bus {
//             Some(bus) => {
//                 let _ = bus.tcp_handle.join();
//                 let _ = bus.udp_handle.join();
//             },
//             None => (),
//         }
//     }

//     pub fn send_msg(&self, msg: &str) {
//         for peer in self.bus.as_ref().unwrap().peers.lock().unwrap().iter() {
//             peer.send_message(msg);
//         }
//     }

}

