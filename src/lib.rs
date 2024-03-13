pub mod ivyerror;
// mod peer;
mod ivy_messages;

use core::fmt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::identity;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream} ; use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
// , UdpSocket
//use std::sync::{Arc};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use crossbeam::select;
use ivy_messages::IvyMsg;
use regex::Regex;
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
struct Peer {
    //joinHandle: std::thread::JoinHandle<()>,
    name: String,
    id: u32,
    subscriptions: Vec<(u32, String)>,
    stream: RwLock<TcpStream>,
    should_terminate: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>
}

impl Peer {
    fn new(peer_id: u32, stream: TcpStream) -> Self {
        Peer {
            name: String::new(),
            id: peer_id,
            subscriptions: vec![],
            stream: RwLock::new(stream),
            should_terminate: Arc::new(AtomicBool::new(false)),
            join_handle: None
        }
    }
}


struct IvyPrivate {
    peers: Vec<Peer>,
    peers_nb: u32,
    client_connected_cb: Option<Box<dyn Fn() + Send + Sync>>,
    subscriptions: HashMap<u32, (String, Box<dyn Fn(&Vec<String>) + Send + Sync>)>,
    should_terminate: Arc<AtomicBool>,
    local_port: u16
}

impl IvyPrivate {
    fn new() -> Self {
        IvyPrivate {
            peers: vec![],
            peers_nb: 0,
            client_connected_cb: None,
            subscriptions: HashMap::new(),
            should_terminate: Arc::new(AtomicBool::new(false)),
            local_port: 0
        }
    }
}

pub struct IvyBus {
    pub appname: String,
    private: Arc<RwLock<IvyPrivate>>,
    snd: Option<Sender<Command>>,
    next_sub_id: AtomicCell<u32>,
}

//#[derive(Debug)]
pub enum Command {
    Sub(u32, String),
    Msg(String),
    DirectMsg(u32, String),
    Quit,
    Stop,
    SetClientConnectedCb(Box<dyn Fn() + Send + Sync>)
}

impl IvyBus {

    pub fn inspect(&self) {
        println!("{self:?}");
    }

    pub fn new(appname: &str) -> Self {
        IvyBus {
            appname: appname.into(),
            private: Arc::new(RwLock::new(IvyPrivate::new())),
            snd: None,
            next_sub_id: AtomicCell::new(0)
        }
    }

    pub fn send(&self, msg: &str) {
        let cmd = Command::Msg(msg.into());
        if let Some(snd) = &self.snd {
            let _ = snd.send(cmd);
        }
    }

    pub fn subscribe(&self, regex: &str, cb: Box<dyn Fn(&Vec<String>) + Send + Sync>) {
        let sub_id = self.next_sub_id.fetch_add(1);
        self.private.write().unwrap().subscriptions.insert(sub_id, (regex.into(), cb));
        let cmd = Command::Sub(sub_id, regex.into());
        if let Some(snd) = &self.snd {
            let _ = snd.send(cmd);
        }
    }

    pub fn set_client_connected_cb(&mut self, cb: Box<dyn Fn() + Send + Sync>) {
        self.private.write().unwrap().client_connected_cb = Some(cb);
    }

    pub fn stop(&self) {
        if let Some(snd) = &self.snd {
            let _ = snd.send(Command::Stop);
        }
        //TODO
        // join all threads

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
        let local_addr = listener.local_addr()?;
        let port = listener.local_addr()?.port();
        self.private.write().unwrap().local_port = port;
        println!("listen on TCP {port}");

        let (sen_tcp, rcv_tcp) = unbounded::<(TcpStream, SocketAddr)>();

        // TCP Listener
        let term = self.private.read().unwrap().should_terminate.clone();
        thread::spawn(move || Self::tcp_listener(listener, sen_tcp, term));


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
        let (sen_cmd, rcv_cmd) = unbounded::<Command>();
        self.snd = Some(sen_cmd);
        let bus_private = self.private.clone();
        let _aa = thread::spawn(move || Self::ivy_loop(bus_private, rcv_tcp, rcv_cmd));

        Ok(())
    }

    fn ivy_loop(bus_private: Arc<RwLock<IvyPrivate>>, rcv_tcp: Receiver<(TcpStream, SocketAddr)>, rcv_cmd: Receiver<Command>) {

        // let mut bp = bus_private.write().unwrap();
        // bp.peers.iter().map(|peer| {
        //     peer.subscriptions.iter().map(|(sub_id, regex)| {
        //         let re = Regex::new(regex).unwrap();
        //         if let Some(caps) = re.captures(haystack) {
        //             caps.iter().map(|sc| println!("{sc:?}"));
        //         }
        //     })
        // });
        //let peer = Peer {name: String::new(), id: peer_id, subscriptions: vec![], stream: RwLock::new(stream), should_terminate: AtomicCell::new(false)};
        
        let (sen_peer, rcv_peer) = unbounded::<(u32, IvyMsg)>();
        loop {
            select! {
                recv(rcv_tcp) -> msg => {
                    match msg {
                        Ok((tcp_stream, addr)) =>  {
                            println!("TCP connection {tcp_stream:?} from {addr:?}");
                            let ss = sen_peer.clone();
                            let mut bp = bus_private.write().unwrap();
                            bp.peers_nb += 1;
                            let peer_id = bp.peers_nb;
                            let stream = tcp_stream.try_clone().unwrap();
                            let peer = Peer::new(peer_id, stream);
                            let term = peer.should_terminate.clone();
                            

                            if let Some(connected_cb) = &bp.client_connected_cb {
                                connected_cb();
                            }
                            
                            bp.peers.push(peer);
                            thread::spawn(move || Self::tcp_read(tcp_stream, ss, peer_id, term));
                            
                        },
                        Err(_error) => todo!(),
                    }
                },
                recv(rcv_peer) -> msg => {
                    match msg {
                        Ok((peer_id, msg)) => {
                            match msg {
                                IvyMsg::Bye => todo!(),
                                IvyMsg::Sub(sub_id, regex) => {
                                    let mut bp = bus_private.write().unwrap();
                                    for peer in &mut bp.peers {
                                        if peer.id == peer_id {
                                            peer.subscriptions.push((sub_id, regex.clone()));
                                        }
                                    }
                                },
                                IvyMsg::TextMsg(sub_id, params) => {
                                    //println!("params:: {params:?}");
                                    let bp = bus_private.write().unwrap();
                                    if let Some((_regex, cb)) = bp.subscriptions.get(&sub_id) {
                                        cb(&params);
                                    }
                                },
                                IvyMsg::Error(_) => todo!(),
                                IvyMsg::DelSub(_) => todo!(),
                                IvyMsg::EndSub => {
                                    let bp = bus_private.write().unwrap();
                                    for peer in &bp.peers {
                                        if peer.id == peer_id {

                                            for (sub_id, (regex, _cb)) in bp.subscriptions.iter() {
                                                let msg = IvyMsg::Sub(*sub_id, regex.clone());
                                                let buf = msg.to_ascii();
                                                let _ = peer.stream.write().unwrap().write(&buf);
                                            }

                                            let msg = IvyMsg::EndSub;
                                            let buf = msg.to_ascii();
                                            let _ = peer.stream.write().unwrap().write(&buf);
                                        }
                                    }
                                },
                                IvyMsg::PeerId(_port, name) => {
                                    let mut bp = bus_private.write().unwrap();
                                    for peer in &mut bp.peers {
                                        if peer.id == peer_id {
                                            peer.name = name.clone();
                                        }
                                    }
                                },
                                IvyMsg::DirectMsg(_, _) => todo!(),
                                IvyMsg::Quit => todo!(),
                                IvyMsg::Ping(_) => todo!(),
                                IvyMsg::Pong(_) => todo!(),
                            }
                        },
                        Err(_) => todo!(),
                    }
                },
                recv(rcv_cmd) -> msg => {
                    match msg {
                        Ok(cmd) => {
                            match cmd {
                                Command::Sub(sub_id, regex) => {
                                    println!("Subscribe to\"{regex}\" with id {sub_id}");
                                    let bp = bus_private.write().unwrap();
                                    for peer in &bp.peers {
                                        let msg = IvyMsg::Sub(sub_id, regex.clone());
                                        let buf = msg.to_ascii();
                                        let _ = peer.stream.write().unwrap().write(&buf);
                                    }
                                },
                                Command::Msg(message) => {
                                    println!("Sending message \"{message}\"");
                                    let bp = bus_private.write().unwrap();
                                    for peer in &bp.peers {
                                        peer.subscriptions.iter().for_each(|(sub_id, regex)| {
                                            // TODO do not recreate the regex each time
                                            let re = Regex::new(regex).unwrap();
                                            println!("{regex}");
                                            if let Some(caps) = re.captures(&message) {
                                                let params = caps.iter()
                                                    .skip(1)
                                                    .filter_map(identity)
                                                    .map(|c| c.as_str().to_string())
                                                    .collect::<Vec<_>>();
                                                let msg = IvyMsg::TextMsg(*sub_id, params);
                                                let buf = msg.to_ascii();
                                                let _ = peer.stream.write().unwrap().write(&buf);
                                            }
                                        })
                                    }
                                },
                                Command::DirectMsg(_id, _msg) => {

                                },
                                Command::Quit => todo!(),
                                Command::Stop => {
                                    let bp = bus_private.write().unwrap();
                                    for peer in &bp.peers {
                                        peer.should_terminate.store(true, Ordering::Release);
                                    }
                                    // TODO
                                    // stop TCP listener
                                    // stop UDP listener
                                    
                                    
                                    let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), bp.local_port);
                                    if TcpStream::connect_timeout(&local_addr, Duration::from_secs(1)).is_ok() {
                                        println!("Listener closing...");
                                    }
                                    
                                    

                                    break;
                                },
                                Command::SetClientConnectedCb(cb) => {
                                    let mut bp = bus_private.write().unwrap();
                                    bp.client_connected_cb = Some(cb);
                                }
                            }
                        },
                        Err(_error) => {
                            //TODO
                        }
                    }
                }

            }
        }
    }

    fn tcp_listener(listener: TcpListener, sen_tcp: Sender<(TcpStream, SocketAddr)>, term: Arc<AtomicBool>) {
        
        loop
        {
            match listener.accept() {
                Ok((tcp_stream, addr)) => {
                    if term.load(Ordering::Acquire) {
                        println!("Exiting TCPListener thread...");
                        break;
                    }
                    //println!("TCP connection {tcp_stream:?} from {addr:?}");
                    let _ = sen_tcp.send((tcp_stream, addr));
                },
                Err(_error) => {

                },
            }
        }
        println!("Exit TCPListener thread");

    }

    fn tcp_read(mut socket: TcpStream, s: Sender<(u32, IvyMsg)>, peer_id: u32, term: Arc<AtomicBool>) {
        let mut buf = vec![0; 1024];
        let _ = socket.set_read_timeout(Some(Duration::from_millis(10)));
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
                Err(_) => (),
            }
            if term.load(Ordering::Acquire) {
                break;
            }
        }
        println!("Exit thread peer {peer_id}")
    }

}




impl fmt::Debug for IvyBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{\"{}\"", self.appname)?;
        //let p = self.private.read().unwrap();
        f.debug_struct("IvyBus")
            .field("appname", &self.appname)
            //.field("private", &format_args!("{:?}", *p))
            .finish()
    }
}


impl fmt::Debug for IvyPrivate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IvyPrivate")
        .field("peers", &self.peers)
        .field("peers_nb", &self.peers_nb)
        .finish()
    }
}