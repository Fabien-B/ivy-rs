pub mod ivyerror;
mod peer;
mod ivy_messages;

use std::{net::{TcpListener, SocketAddr, TcpStream}, fmt::write, str::FromStr, io::Read, thread::{self, JoinHandle}, sync::{Arc, Mutex}};
use socket2::{Socket, Domain, Type, Protocol, SockAddr};
use ivyerror::IvyError;
use peer::Peer;
use std::time::Duration;

const PROTOCOL_VERSION: u32 = 3;



pub struct IvyBus {
    pub appname: String,
    bus: Option<BusPrivate>
}

struct BusPrivate {
    domain: String,
    //subscriptions: Vec<(String, Fn)>,
    udp_handle: JoinHandle<()>,
    tcp_handle: JoinHandle<()>,
    ////tcp_listener: TcpListener,
    //listener: std::thread::JoinHandle<_>,
    watcher_id: String,
    peers: Arc<Mutex<Vec<Peer>>>,
}

impl IvyBus {

    pub fn new(appname: &str) -> Self {
        IvyBus {appname: appname.to_string(), bus: None}
    }

    pub fn start(&mut self, domain: &str) -> Result<(), IvyError> {
        // domain: 127.255.255.255:2010
        // Check if domain is correctly formed, then connect to bus.
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        //let address: SocketAddr = "0.0.0.0:2010".parse().unwrap();
        let address = address.into();
        udp_socket.set_reuse_address(true)?;
        udp_socket.bind(&address)?;
        udp_socket.set_broadcast(true)?;

        let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
        let ta: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let ta = ta.into();
        socket.bind(&ta)?;
        socket.set_reuse_address(true).unwrap();
        //socket.bind(&ta)?;
        socket.listen(128)?;

        let tcp_listener: TcpListener = socket.into();




        let port = tcp_listener.local_addr()?.port();
        println!("listen on TCP port {}", port);
        // watcherId to be sure no to treat our own announcement message.
        let watcher_id = format!("{}_{}", self.appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{} {} {} {}\n", PROTOCOL_VERSION, port, watcher_id, self.appname);
        udp_socket.send_to(announce.as_bytes(), &address)?;

        let udp_handle = std::thread::spawn(move || {
            // udp_socket blocking listen
            // with mpsc channel to stop it
        });

        let peers = Arc::new(Mutex::new(Vec::new()));

        let tcpeers = peers.clone();

        let tcp_handle = std::thread::spawn(move || {
            // TODO mpsc channel to stop it
            for stream in tcp_listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let p = Peer::handle_incoming(stream);
                        tcpeers.lock().unwrap().push(p);
                    }
                    Err(e) => { println!("fail: {:?}", e);}
                }
        
            }
        });

        self.bus = BusPrivate{domain: domain.to_string(), udp_handle, tcp_handle, watcher_id, peers}.into();
        
        Ok(())
    }

    pub fn join(self) {
        match self.bus {
            Some(bus) => {
                let _ = bus.tcp_handle.join();
                let _ = bus.udp_handle.join();
            },
            None => (),
        }
    }

    pub fn send_msg(&self, msg: &str) {
        for peer in self.bus.as_ref().unwrap().peers.lock().unwrap().iter() {
            peer.send_message(msg);
        }
    }

}

