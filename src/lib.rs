pub mod ivyerror;
// mod peer;
mod ivy_messages;

use std::borrow::{Borrow, BorrowMut};
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream} ; use std::sync::{Arc, Mutex};
// , UdpSocket
//use std::sync::{Arc};
use std::thread;
use crossbeam::select;
use ivy_messages::IvyMsg;
//use std::time::Duration;
use socket2::{Socket, Domain, Type, Protocol};
use ivyerror::IvyError;
//use crate::ivy_messages::IvyMsg;
// use peer::Peer;
// use std::time::Duration;
use crossbeam::channel::{unbounded, Receiver, Sender};
use crossbeam::atomic::AtomicCell;



const PROTOCOL_VERSION: u32 = 3;

#[derive(Debug)]
struct IvyPrivate {
    peers: Vec<String>,
    peers_nb: u32,
}

#[derive(Debug)]
pub struct IvyBus {
    pub appname: String,
    private: Arc<Mutex<IvyPrivate>>
}


impl IvyBus {

    pub fn inspect(&self) {
        print!("{self:?}");
    }

    pub fn new(appname: &str) -> Self {
        IvyBus {
            appname: appname.into(),
            private: Arc::new(Mutex::new(IvyPrivate { peers: vec![], peers_nb: 0 }))
        }
    }

    pub fn start_ivy_loop(&mut self, domain: &str) -> Result<(), IvyError> {
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        udp_socket.set_reuse_address(true)?;
        udp_socket.set_broadcast(true)?;
        udp_socket.set_nonblocking(true)?;
        udp_socket.bind(&address.into())?;


        //let listener = TcpListener::bind("0.0.0.0:0")?;
        let listener = TcpListener::bind("0.0.0.0:8888")?;
        let port = listener.local_addr()?.port();
        println!("listen on TCP {port}");

        let (sen_tcp, rcv_tcp) = unbounded::<(TcpStream, SocketAddr)>();

        // TCP Listener
        thread::spawn(move || Self::tcp_listener(listener, sen_tcp));


        // Send UDP annoucement
        // watcherId to be sure no to treat our own announcement message.
        let watcher_id = format!("{}_{}", self.appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{PROTOCOL_VERSION} {port} {watcher_id} {}\n", self.appname);
        let _ = udp_socket.send_to(announce.as_bytes(), &address.into());

        // UDP Listener for new peers
        thread::spawn(move || {
            loop
            {
                let mut _buf:[u8; 1024];
                // match udp_socket.recv_from(&mut buf) {
                //     Ok((n, src)) => {
                //         println!("UDP received {n} bytes from {src:?}");
                //         let rcv_str = String::from_utf8(buf[0..n].to_owned()).unwrap();
                //         if announce == rcv_str {
                //             println!("received back sended annouce");
                //         } else {
                //             println!("UDP rcv: {rcv_str}");
                //         }
                //     },
                //     Err(error) => {

                //     },
                // }
            }

        });

        // main Ivy loop
        let bus_private = self.private.clone();
        let aa = thread::spawn(move || Self::ivy_loop(bus_private, rcv_tcp));

        Ok(())
    }

    fn ivy_loop(bus_private: Arc<Mutex<IvyPrivate>>, rcv_tcp: Receiver<(TcpStream, SocketAddr)>) {
        
        
        let (sen_peer, rcv_peer) = unbounded::<(u32, IvyMsg)>();
        loop {
            select! {
                recv(rcv_tcp) -> msg => {
                    match msg {
                        Ok((tcp_stream, addr)) =>  {
                            println!("TCP connection {tcp_stream:?} from {addr:?}");
                            let ss = sen_peer.clone();
                            let mut bp = bus_private.lock().unwrap();
                            bp.peers_nb += 1;
                            let peer_id = bp.peers_nb;
                            thread::spawn(move || Self::tcp_read(tcp_stream, ss, peer_id));
                        },
                        Err(_error) => todo!(),
                    }
                },
                recv(rcv_peer) -> msg => {
                    match msg {
                        Ok((peer_id, msg)) => {
                            println!("{msg:?}");
                            match msg {
                                IvyMsg::Bye => todo!(),
                                IvyMsg::Sub(_, _) => todo!(),
                                IvyMsg::TextMsg(_, _) => todo!(),
                                IvyMsg::Error(_) => todo!(),
                                IvyMsg::DelSub(_) => todo!(),
                                IvyMsg::EndSub => {},
                                IvyMsg::PeerId(port, name) => {
                                    let mut aa = bus_private.lock().unwrap();
                                    aa.peers.push(format!("{peer_id}: {name}"));
                                },
                                IvyMsg::DirectMsg(_, _) => todo!(),
                                IvyMsg::Quit => todo!(),
                                IvyMsg::Ping(_) => todo!(),
                                IvyMsg::Pong(_) => todo!(),
                            }
                        },
                        Err(_) => todo!(),
                    }
                }

            }
        }
    }

    fn tcp_listener(listener: TcpListener, sen_tcp: Sender<(TcpStream, SocketAddr)>) {
        loop
        {
            match listener.accept() {
                Ok((tcp_stream, addr)) => {
                    //println!("TCP connection {tcp_stream:?} from {addr:?}");
                    let _ = sen_tcp.send((tcp_stream, addr));
                },
                Err(_error) => {

                },
            }
        }

    }

    fn tcp_read(mut socket: TcpStream, s: Sender<(u32, IvyMsg)>, peer_id: u32) {
        let mut buf = vec![0; 1024];
        loop {
            match socket.read(&mut buf) {
                Ok(n) => {
                    let lines = buf[0..n]
                    .split(|c| *c==b'\n')
                    .filter(|buf| buf.len() > 0);
                    for line in lines {
                        let msg = IvyMsg::parse(line);
                        if let Ok(msg) = msg {
                            let _ = s.send((peer_id, msg));
                        }
                    }
                },
                Err(_) => todo!(),
            }
        }
    }

}

