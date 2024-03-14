pub mod ivyerror;
// mod peer;
mod ivy_messages;

use core::fmt;
use std::collections::HashMap;
use std::convert::identity;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream} ; use std::sync::atomic::{AtomicBool, Ordering};
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

const TCPLISTENER_THD_ID: u32 = 0;
const UDPLISTENER_THD_ID: u32 = 1;

#[derive(Debug)]
struct Peer {
    //joinHandle: std::thread::JoinHandle<()>,
    name: String,
    subscriptions: Vec<(u32, String)>,
    stream: RwLock<TcpStream>,
    should_terminate: Arc<AtomicBool>,
    //join_handle: Option<JoinHandle<()>>
}

impl Peer {
    fn new(stream: TcpStream) -> Self {
        Peer {
            name: String::new(),
            subscriptions: vec![],
            stream: RwLock::new(stream),
            should_terminate: Arc::new(AtomicBool::new(false)),
            //join_handle: None
        }
    }
}


struct PeerData {
    socket: TcpStream,
    s: Sender<(u32, IvyMsg)>,
    peer_id: u32,
    term: Arc<AtomicBool>,
    s_pd: Sender<u32>,
}

struct IvyPrivate {
    peers: HashMap<u32, Peer>,
    next_thread_id: AtomicCell<u32>,
    client_connected_cb: Option<Box<dyn Fn() + Send + Sync>>,
    subscriptions: HashMap<u32, (String, Box<dyn Fn(&Vec<String>) + Send + Sync>)>,
    should_terminate: Arc<AtomicBool>,
    local_port: u16,
    ivy_thd_handle: Option<JoinHandle<()>>,
    join_handles: HashMap<u32, JoinHandle<()>>
}

impl IvyPrivate {
    fn new() -> Self {
        IvyPrivate {
            peers: HashMap::new(),
            next_thread_id: AtomicCell::new(2),
            client_connected_cb: None,
            subscriptions: HashMap::new(),
            should_terminate: Arc::new(AtomicBool::new(false)),
            local_port: 0,
            ivy_thd_handle: None,
            join_handles: HashMap::new()
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
        if let Some(snd) = &self.snd {
            let _ = snd.send(Command::Msg(msg.into()));
        }
    }

    pub fn subscribe(&self, regex: &str, cb: Box<dyn Fn(&Vec<String>) + Send + Sync>) {
        if let Some(snd) = &self.snd {
            let sub_id = self.next_sub_id.fetch_add(1);
            self.private.write().unwrap().subscriptions.insert(sub_id, (regex.into(), cb));
            let cmd = Command::Sub(sub_id, regex.into());
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

        // join the ivy thread: it exits only when all others threads are terminated
        let ivy_handle = self.private.write().unwrap().ivy_thd_handle.take();
        if let Some(handle) = ivy_handle {
            let _ = handle.join();
            println!("ivy thread terminated");
        }

    }

    pub fn start_ivy_loop(&mut self, domain: &str) -> Result<(), IvyError> {
        // create UDP socket
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        udp_socket.set_reuse_address(true)?;
        udp_socket.set_broadcast(true)?;
        udp_socket.set_nonblocking(true)?;
        udp_socket.bind(&address.into())?;


        // create TCPListener and bind it 
        //let listener = TcpListener::bind("0.0.0.0:0")?;
        let listener = TcpListener::bind("0.0.0.0:8888")?;
        let port = listener.local_addr()?.port();
        self.private.write().unwrap().local_port = port;

        // channel for new TCP connections
        let (snd_tcp, rcv_tcp) = unbounded::<(TcpStream, SocketAddr)>();

        // channels for thread terminaisons
        let (snd_thd_terminated, rcv_thd_terminated) = unbounded::<u32>();

        // TCP Listener
        let tcp_term_flag = self.private.read().unwrap().should_terminate.clone();
        let tcp_snd_term = snd_thd_terminated.clone();
        let tcplistener_handle =
            thread::spawn(move ||
                Self::tcp_listener(listener,
                    snd_tcp, 
                    tcp_term_flag, 
                    tcp_snd_term)
            );
        //TODO add thread handle to hashmap
        //self.private.write().unwrap().join_handles.insert(TCPLISTENER_THD_ID, tcplistener_handle);


        // Send UDP annoucement
        let watcher_id = format!("{}_{}", self.appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{PROTOCOL_VERSION} {port} {watcher_id} {}\n", self.appname);
        let _ = udp_socket.send_to(announce.as_bytes(), &address.into());

        // channels for thread terminaisons
        let udp_snd_term = snd_thd_terminated.clone();
        // UDP Listener for new peers
        let udplistener_handle = thread::spawn(move || {
            loop
            {
                let mut _buf:[u8; 1024];
                // TODO
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
                break;
            }
            let _ = udp_snd_term.send(UDPLISTENER_THD_ID);

        });
        self.private.write().unwrap().join_handles.insert(UDPLISTENER_THD_ID, udplistener_handle);

        // main Ivy loop
        // channels for commands (client thread to Ivy thread)
        let (sen_cmd, rcv_cmd) = unbounded::<Command>();
        self.snd = Some(sen_cmd);
        let bus_private = self.private.clone();
        let ivy_handle = thread::spawn(move ||
            Self::ivy_loop(bus_private,
                rcv_tcp,
                rcv_cmd,
                snd_thd_terminated,
                rcv_thd_terminated)
        );
        self.private.write().unwrap().ivy_thd_handle = Some(ivy_handle);

        Ok(())
    }

    fn ivy_loop(bus_private: Arc<RwLock<IvyPrivate>>, rcv_tcp: Receiver<(TcpStream, SocketAddr)>, rcv_cmd: Receiver<Command>,
    snd_thd_terminated: Sender<u32>, rcv_thd_terminated: Receiver<u32>) {

        let (sen_peer, rcv_peer) = unbounded::<(u32, IvyMsg)>();

        // let mut bp = bus_private.write().unwrap();

        loop {
            select! {
                recv(rcv_tcp) -> msg => {
                    match msg {
                        Ok((tcp_stream, addr)) =>  {
                            println!("TCP connection {tcp_stream:?} from {addr:?}");
                            let mut bp = bus_private.write().unwrap();
                            let peer_id = bp.next_thread_id.fetch_add(1);
                            let stream = tcp_stream.try_clone().unwrap();
                            let peer = Peer::new(stream);

                            let pd = PeerData{
                                socket: tcp_stream,
                                s: sen_peer.clone(),
                                peer_id,
                                term: peer.should_terminate.clone(),
                                s_pd: snd_thd_terminated.clone(),
                            };

                            let handle = thread::spawn(move || Self::tcp_read(pd));
                            bp.join_handles.insert(peer_id, handle);
                            bp.peers.insert(peer_id, peer);

                            if let Some(connected_cb) = &bp.client_connected_cb {
                                connected_cb();
                            }
                            
                        },
                        Err(_error) => todo!(),
                    }
                },
                recv(rcv_peer) -> msg => {
                    if let Ok((peer_id, msg)) = msg {
                        Self::handle_ivymsg(peer_id, &msg, &bus_private);
                    }
                },
                recv(rcv_cmd) -> cmd => {
                    if let Ok(cmd) = cmd {
                        Self::handle_command(cmd, &bus_private);
                    }
                },
                recv(rcv_thd_terminated) -> msg => {
                    if let Ok(thread_id) = msg {
                        let mut bp = bus_private.write().unwrap();
                        // join the thread
                        if let Some(handle) = bp.join_handles.remove(&thread_id) {
                            let _ = handle.join();
                        }

                        // remove the associated peer (if it's a peer thread)
                        if let Some(peer) = bp.peers.remove(&thread_id) {
                            println!("droping peer {}", peer.name);   
                        }

                        // exit the ivy thread if:
                        //  - all threads have been joined,
                        //  - the terminate flag is set
                        if bp.join_handles.is_empty() && bp.should_terminate.load(Ordering::Acquire){
                            break;
                        }
                        
                    }
                }

            }
        }
    }

    fn handle_ivymsg(peer_id: u32, msg: &IvyMsg, bus_private: &Arc<RwLock<IvyPrivate>>) {
        match msg {
            IvyMsg::Bye => {
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.should_terminate.store(true, Ordering::Release);
                }
            },
            IvyMsg::Sub(sub_id, regex) => {
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.subscriptions.push((*sub_id, regex.clone()));
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
                if let Some(peer) = bp.peers.get(&peer_id) {
                    for (sub_id, (regex, _cb)) in bp.subscriptions.iter() {
                        let msg = IvyMsg::Sub(*sub_id, regex.clone());
                        let buf = msg.to_ascii();
                        let _ = peer.stream.write().unwrap().write(&buf);
                    }

                    let msg = IvyMsg::EndSub;
                    let buf = msg.to_ascii();
                    let _ = peer.stream.write().unwrap().write(&buf);
                }
            },
            IvyMsg::PeerId(_port, name) => {
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.name = name.clone();
                }
            },
            IvyMsg::DirectMsg(_, _) => todo!(),
            IvyMsg::Quit => todo!(),
            IvyMsg::Ping(_) => todo!(),
            IvyMsg::Pong(_) => todo!(),
        }
    }

    fn handle_command(cmd: Command, bus_private: &Arc<RwLock<IvyPrivate>>) {
        match cmd {
            Command::Sub(sub_id, regex) => {
                println!("Subscribe to\"{regex}\" with id {sub_id}");
                let bp = bus_private.write().unwrap();
                for peer in bp.peers.values() {
                    let msg = IvyMsg::Sub(sub_id, regex.clone());
                    let buf = msg.to_ascii();
                    let _ = peer.stream.write().unwrap().write(&buf);
                }
            },
            Command::Msg(message) => {
                println!("Sending message \"{message}\"");
                let bp = bus_private.write().unwrap();
                for peer in bp.peers.values() {
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
                // call dedicated callback
                todo!()
            },
            Command::Quit => {
                // vasistas ?
                todo!()
            },
            Command::Stop => {
                let bp = bus_private.write().unwrap();
                bp.should_terminate.store(true, Ordering::Release);
                
                // try trigger TCPListener
                // let local_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), bp.local_port);
                // if TcpStream::connect_timeout(&local_addr, Duration::from_secs(1)).is_ok() {
                //     println!("Listener closing...");
                // }

                //TODO
                // join TCPlistener, UDPListener

                for peer in bp.peers.values() {
                    peer.should_terminate.store(true, Ordering::Release);
                }

                //break;
            },
            Command::SetClientConnectedCb(cb) => {
                let mut bp = bus_private.write().unwrap();
                bp.client_connected_cb = Some(cb);
            }
        }
    }

    fn tcp_listener(listener: TcpListener, sen_tcp: Sender<(TcpStream, SocketAddr)>, term: Arc<AtomicBool>,
                    snd_thd_terminated: Sender<u32>) {
        
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
        let _ = snd_thd_terminated.send(TCPLISTENER_THD_ID);
        println!("Exit TCPListener thread");

    }

    fn tcp_read(mut data: PeerData) {
        let mut buf = vec![0; 1024];
        let _ = data.socket.set_read_timeout(Some(Duration::from_millis(10)));
        loop {
            match data.socket.read(&mut buf) {
                Ok(n) => {
                    let lines = buf[0..n]
                    .split(|c| *c==b'\n')
                    .filter(|buf| buf.len() > 0);
                    for line in lines {
                        let msg = IvyMsg::parse(line);
                        if let Ok(msg) = msg {
                            let _ = data.s.send((data.peer_id, msg));
                        }
                    }
                },
                Err(_) => (),
            }
            if data.term.load(Ordering::Acquire) {
                break;
            }
        }
        let _ = data.s_pd.send(data.peer_id);
        println!("Exit thread peer {}", data.peer_id);
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
        //.field("peers_nb", &self.peers_nb)
        .finish()
    }
}