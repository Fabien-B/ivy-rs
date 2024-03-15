pub mod ivyerror;
mod ivy_messages;
mod peer;

use core::fmt;
use std::collections::HashMap;
use std::convert::identity;
use std::io::Write;
use std::mem::MaybeUninit;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use crossbeam::select;
use regex::Regex;
use socket2::{Domain, Protocol, Socket, Type};
use crossbeam::channel::{unbounded, Receiver, Sender};
use crossbeam::atomic::AtomicCell;

use ivy_messages::{IvyMsg, parse_udp_announce};
use ivyerror::IvyError;
use peer::{Peer, PeerData, tcp_read};

const PROTOCOL_VERSION: u32 = 3;

const TCPLISTENER_THD_ID: u32 = 0;
const UDPLISTENER_THD_ID: u32 = 1;

type IvyCb = Box<dyn Fn(&Peer, Vec<String>) + Send + Sync>;
type IvyPeerConnectedCb = Box<dyn Fn(&Peer) + Send + Sync>;
type IvyDirectMessageCb = Box<dyn Fn(&Peer, u32, String) + Send + Sync>;
type IvyQuitCb = Box<dyn Fn(&Peer) + Send + Sync>;

struct IvyPrivate {
    appname: String,
    peers: HashMap<u32, Peer>,
    next_thread_id: AtomicCell<u32>,
    client_connected_cb: Option<IvyPeerConnectedCb>,
    direct_message_cb: Option<IvyDirectMessageCb>,
    quit_cb: Option<IvyQuitCb>,
    subscriptions: HashMap<u32, (String, IvyCb)>,
    should_terminate: Arc<AtomicBool>,
    local_port: u16,
    ivy_thd_handle: Option<JoinHandle<()>>,
    join_handles: HashMap<u32, JoinHandle<()>>,
    ping_data: HashMap<u32, (Sender<Duration>, Instant)>,
}

impl IvyPrivate {
    fn new(appname: String) -> Self {
        IvyPrivate {
            appname,
            peers: HashMap::new(),
            next_thread_id: AtomicCell::new(2),
            client_connected_cb: None,
            direct_message_cb: None,
            quit_cb: None,
            subscriptions: HashMap::new(),
            should_terminate: Arc::new(AtomicBool::new(false)),
            local_port: 0,
            ivy_thd_handle: None,
            join_handles: HashMap::new(),
            ping_data: HashMap::new()
        }
    }
}

pub struct IvyBus {
    private: Arc<RwLock<IvyPrivate>>,
    snd: Option<Sender<Command>>,
    next_sub_id: AtomicCell<u32>,
    next_ping_id: AtomicCell<u32>,
}

/// Commands from application to Ivy thread
enum Command {
    /// Subscribe to a regex
    Sub(u32, String),
    /// Send message to peers (matching)
    SendMsg(String),
    /// Send a direct message to a peer (unique peer, no matching)
    /// (peer_id, id, msg)
    SendDirectMsg(u32, u32, String),
    /// Unsubscribe from a regex
    UnSub(u32),
    /// Ping peer
    /// peer_id, ping_id, channel(time)
    Ping(u32, u32, Sender<Duration>),
    /// Stop Ivy
    Stop,
}

impl IvyBus {

    pub fn new(appname: &str) -> Self {
        IvyBus {
            private: Arc::new(RwLock::new(IvyPrivate::new(appname.into()))),
            snd: None,
            next_sub_id: AtomicCell::new(0),
            next_ping_id: AtomicCell::new(0),
        }
    }

    /// Send text message (matching)
    pub fn send(&self, msg: &str) {
        if let Some(snd) = &self.snd {
            let _ = snd.send(Command::SendMsg(msg.into()));
        }
    }

    /// Send Direct message to a peer
    pub fn send_direct_msg(&self, peer_id: u32, id:u32, msg: String) {
        if let Some(snd) = &self.snd {
            let _ = snd.send(Command::SendDirectMsg(peer_id, id, msg));
        }
    }

    /// Subscribe to a regex
    /// # Parameters
    /// `regex`: the regex to match messages against
    /// 
    /// `cb`: A callback of the form `fn(peer: &Peer, params: Vec<String>)`
    /// # Return value
    /// Returns a `sub_id: u32` that can be used to unsubscribe from that regex
    pub fn subscribe(&self, regex: &str, cb: IvyCb) -> u32 {
        let mut bp = self.private.write().unwrap();
        let sub_id = self.next_sub_id.fetch_add(1);
        bp.subscriptions.insert(sub_id, (regex.into(), cb));

        // if ivy loop is running, send a message
        if let Some(snd) = &self.snd {
            let cmd = Command::Sub(sub_id, regex.into());
            let _ = snd.send(cmd);
        }
        // return subscription id
        sub_id
    }

    /// Unsubscribe from a regex
    /// # Arguments
    /// `sub_id: u32`: regex identifier, as returned by the `subscribe` method
    pub fn unsubscribe(&self, sub_id: u32) {
        let mut bp = self.private.write().unwrap();
        bp.subscriptions.remove(&sub_id);

        // if ivy loop is running, send a message
        if let Some(snd) = &self.snd {
            let cmd = Command::UnSub(sub_id);
            let _ = snd.send(cmd);
        }
    }

    pub fn ping(&self, peer_id: u32, timeout: Duration) -> Result<Duration, IvyError> {
        if let Some(snd) = &self.snd {
            let (ping_snd, ping_rcv) = unbounded::<Duration>();
            let ping_id = self.next_ping_id.fetch_add(1);
            let _ = snd.send(Command::Ping(peer_id, ping_id, ping_snd));
            match ping_rcv.recv_timeout(timeout) {
                Ok(d) => Ok(d),
                Err(_) => Err(IvyError::PingTimeout),
            }
        } else {
            Err(IvyError::BadInit)
        }
    }

    /// Set a callback that will be called when a new peer is connected
    pub fn set_client_connected_cb(&mut self, cb: IvyPeerConnectedCb) {
        self.private.write().unwrap().client_connected_cb = Some(cb);
    }

    /// Set a callback that will be called on direct message
    pub fn set_direct_message_cb(&mut self, cb: IvyDirectMessageCb) {
        self.private.write().unwrap().direct_message_cb = Some(cb);
    }

    /// Set a callback that will be called on quit request
    pub fn set_quit_cb(&mut self, cb: IvyQuitCb) {
        self.private.write().unwrap().quit_cb = Some(cb);
    }

    /// Stop the bus
    pub fn stop(&self) {
        if let Some(snd) = &self.snd {
            let _ = snd.send(Command::Stop);
        }

        // join the ivy thread: it exits only when all others threads are terminated
        let ivy_handle = self.private.write().unwrap().ivy_thd_handle.take();
        if let Some(handle) = ivy_handle {
            let _ = handle.join();
        }

    }

    /// Start Ivy
    pub fn start(&mut self, domain: &str) -> Result<(), IvyError> {
        // create TCPListener and bind it 
        let listener = TcpListener::bind("0.0.0.0:0")?;
        let port = listener.local_addr()?.port();
        self.private.write().unwrap().local_port = port;

        // channel for new TCP connections
        let (snd_tcp, rcv_tcp) = unbounded::<(TcpStream, SocketAddr)>();

        // channel for new UDP client
        let (snd_udp, rcv_udp) = unbounded::<(SocketAddr, String)>();

        // channels for thread terminaisons
        let (snd_thd_terminated, rcv_thd_terminated) = unbounded::<u32>();

        // TCP Listener
        let tcp_term_flag = self.private.read().unwrap().should_terminate.clone();
        let tcp_snd_term = snd_thd_terminated.clone();
        // TODO do something with the join handle !
        let _tcplistener_handle =
            thread::spawn(move ||
                Self::tcp_listener(listener,
                    snd_tcp, 
                    tcp_term_flag, 
                    tcp_snd_term)
            );
        //TODO add thread handle to hashmap
        //self.private.write().unwrap().join_handles.insert(TCPLISTENER_THD_ID, tcplistener_handle);

        // channel for thread terminaisons
        let udp_snd_term = snd_thd_terminated.clone();
        // terminate flag
        let udp_term_flag = self.private.read().unwrap().should_terminate.clone();
        // domain and appname to pass to UDP thread
        let appname_udp = self.private.read().unwrap().appname.clone();
        let domain_udp = domain.to_string();
        // UDP Listener for new peers
        let udplistener_handle = thread::spawn(move || Self::udp_listener(domain_udp, appname_udp, port, udp_snd_term, udp_term_flag, snd_udp));
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
                rcv_udp,
                snd_thd_terminated,
                rcv_thd_terminated)
        );
        self.private.write().unwrap().ivy_thd_handle = Some(ivy_handle);

        Ok(())
    }


//////////////////////////////////////////////////
/// Private methods
//////////////////////////////////////////////////


    fn make_udp_socket(domain: &str) -> Result<(Socket, SocketAddr), IvyError> {
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        let address: SocketAddr = domain.parse().unwrap();
        udp_socket.set_reuse_address(true)?;
        udp_socket.set_broadcast(true)?;
        let _ = udp_socket.set_read_timeout(Some(Duration::from_millis(10)));
        udp_socket.bind(&address.into())?;
        Ok((udp_socket, address))
    }


    fn udp_listener(
            domain: String,
            appname: String,
            port: u16,
            snd_term: Sender<u32>,
            term: Arc<AtomicBool>,
            snd_udp: Sender<(SocketAddr, String)>) {

        let (udp_socket, address) = Self::make_udp_socket(&domain).unwrap();
        // Send UDP annoucement
        let my_watcher_id = format!("{}_{}", appname, port);
        // <protocol version> <TCP port> <watcherId> <application name>
        let announce = format!("{PROTOCOL_VERSION} {port} {my_watcher_id} {}\n", appname);
        let _ = udp_socket.send_to(announce.as_bytes(), &address.into());


        let mut _buf = [MaybeUninit::uninit();1024];
        
        loop
        {
            if let Ok((n, src)) = udp_socket.recv_from(&mut _buf) {
                // DANGER! Do not read the buffer past the nth element !
                // Assume all array is initialized: this is false!
                // See socket2 recv method : promise it's fine for the [0..n] slice
                let udp_frame = unsafe { std::mem::transmute::<_, &[u8]>(_buf.as_slice()) };
                let udp_frame = udp_frame[0..n].to_owned();
                let rcv_str = String::from_utf8(udp_frame).unwrap();
                let parsed = parse_udp_announce(&rcv_str);
                if let Ok(ret)  = parsed {
                    let (protocol_version, port, watcher_id, peer_name) = ret;
                    if protocol_version != PROTOCOL_VERSION {
                        continue;
                    }
                    if watcher_id == my_watcher_id {
                        // my own announce, ignoring it
                        continue;
                    }
                    
                    // send (port, peer_name) to ivy thread
                    if let Some(addr) = src.as_socket() {
                        let ip = addr.ip();
                        let addr = SocketAddr::new(ip, port);
                        let _ = snd_udp.send((addr, peer_name));
                    }
                }
            }

            if term.load(Ordering::Acquire) {
                break;
            }
        }
        let _ = snd_term.send(UDPLISTENER_THD_ID);
    }

    fn send_initial_subscriptions(stream: &mut TcpStream, subscriptions: &HashMap<u32, (String, IvyCb)>) {
        // send Initial Subscriptions
        for (sub_id, (regex, _)) in subscriptions.iter() {
            let msg = IvyMsg::Sub(*sub_id, regex.clone());
            let _ = stream.write(&msg.to_ascii());
        }
        // send end of initial subsciption
        let buf = IvyMsg::EndSub.to_ascii();
        let _ = stream.write(&buf);
    }

    fn ivy_loop(bus_private: Arc<RwLock<IvyPrivate>>, rcv_tcp: Receiver<(TcpStream, SocketAddr)>, rcv_cmd: Receiver<Command>, rcv_udp: Receiver<(SocketAddr, String)>,
    snd_thd_terminated: Sender<u32>, rcv_thd_terminated: Receiver<u32>) {

        let (sen_peer, rcv_peer) = unbounded::<(u32, IvyMsg)>();

        loop {
            select! {
                // new TCP connection
                recv(rcv_tcp) -> msg => {
                    if let Ok((tcp_stream, _addr)) = msg {
                        Self::handle_tcp_connection(tcp_stream, &snd_thd_terminated, &bus_private, &sen_peer);
                    }
                },
                // message from peer
                recv(rcv_peer) -> msg => {
                    if let Ok((peer_id, msg)) = msg {
                        Self::handle_ivymsg(peer_id, msg, &bus_private);
                    }
                },
                // command from application
                recv(rcv_cmd) -> cmd => {
                    if let Ok(cmd) = cmd {
                        Self::handle_command(cmd, &bus_private);
                    }
                },
                // new UDP message (new peer incoming)
                recv(rcv_udp) -> udp_msg => {
                    if let Ok((addr, peer_name)) = udp_msg {
                        Self::handle_udp(&bus_private, addr, peer_name, &sen_peer, &snd_thd_terminated);
                    }
                }
                // notification thread termination
                recv(rcv_thd_terminated) -> msg => {
                    if let Ok(thread_id) = msg {
                        Self::handle_terminated(thread_id, &bus_private);
                    }
                }
            }
        }
    }

    fn handle_terminated(thread_id: u32, bus_private: &Arc<RwLock<IvyPrivate>>) -> bool {
        let mut bp = bus_private.write().unwrap();
        // join the thread
        if let Some(handle) = bp.join_handles.remove(&thread_id) {
            let _ = handle.join();
        }

        // remove the associated peer (if it's a peer thread)
        if let Some(_peer) = bp.peers.remove(&thread_id) {
            //println!("droping peer {}", peer.name);   
        }

        // exit the ivy thread if:
        //  - all threads have been joined,
        //  - the terminate flag is set
        if bp.join_handles.is_empty() && bp.should_terminate.load(Ordering::Acquire){
            true
        } else {
            false
        }
    }

    /// Handle new TCP connection:
    /// add new peer and spawn new peer thread
    fn handle_tcp_connection(
            tcp_stream: TcpStream, snd_thd_terminated: &Sender<u32>,
            bus_private: &Arc<RwLock<IvyPrivate>>,
            sen_peer: &Sender<(u32, IvyMsg)>) {
        let mut bp = bus_private.write().unwrap();
        let peer_id = bp.next_thread_id.fetch_add(1);
        let stream = tcp_stream.try_clone().unwrap();
        let mut stream_init_subs = tcp_stream.try_clone().unwrap();
        let peer = Peer::new(stream);

        let pd = PeerData {
            socket: tcp_stream,
            snd_ivymsg: sen_peer.clone(),
            peer_id,
            term: peer.should_terminate.clone(),
            snd_thd_terminated: snd_thd_terminated.clone(),
        };

        let handle = thread::spawn(move || tcp_read(pd));
        bp.join_handles.insert(peer_id, handle);

        bp.peers.insert(peer_id, peer);
        Self::send_initial_subscriptions(&mut stream_init_subs, &bp.subscriptions);
    }

    /// Handle messages coming from a peer socket
    /// 
    /// See the details of the messages here: https://ivybus.gitlab.io/protocol_messages.html#messages
    fn handle_ivymsg(peer_id: u32, msg: IvyMsg, bus_private: &Arc<RwLock<IvyPrivate>>) {
        match msg {
            IvyMsg::Bye => {
                // terminate peer thread
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.should_terminate.store(true, Ordering::Release);
                }
            },
            IvyMsg::Sub(sub_id, regex) => {
                // Add a regex to a peer's subscriptions
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.subscriptions.insert(sub_id, regex.clone());
                }
            },
            IvyMsg::TextMsg(sub_id, params) => {
                // call the callback associated with the subscription
                let bp = bus_private.write().unwrap();
                if let Some((_regex, cb)) = bp.subscriptions.get(&sub_id) {
                    if let Some(peer) = bp.peers.get(&peer_id) {
                        cb(&peer, params);
                        //TODO execute in a thread ?
                        //thread::spawn(move || cb(params));
                    }
                }
            },
            IvyMsg::Error(_) => todo!(),
            IvyMsg::DelSub(sub_id) => {
                // remove regex from a peer
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.subscriptions.remove(&sub_id);
                }
            },
            IvyMsg::EndSub => {
                // Peer connected and initialized
                let bp = bus_private.read().unwrap();
                if let Some(connected_cb) = &bp.client_connected_cb {
                    if let Some(peer) = bp.peers.get(&peer_id) {
                        connected_cb(&peer);
                    }
                }
            },
            IvyMsg::PeerId(_port, name) => {
                // identify the peer
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    peer.name = name.clone();
                }
            },
            IvyMsg::DirectMsg(id, msg) => {
                // call the direct message callback
                let bp = bus_private.read().unwrap();
                if let Some(direct_message_cb) = &bp.direct_message_cb {
                    if let Some(peer) = bp.peers.get(&peer_id) {
                        direct_message_cb(&peer, id, msg);
                    }
                }
            },
            IvyMsg::Quit => {
                // call the quit callback
                let bp = bus_private.read().unwrap();
                if let Some(quit_cb) = &bp.quit_cb {
                    if let Some(peer) = bp.peers.get(&peer_id) {
                        quit_cb(&peer);
                    }
                }
            },
            IvyMsg::Ping(ping_id) => {
                // respond with pong
                let mut bp = bus_private.write().unwrap();
                if let Some(peer) = bp.peers.get_mut(&peer_id) {
                    let msg = IvyMsg::Pong(ping_id);
                    let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                }
            },
            IvyMsg::Pong(_pong_id) => {
                let mut bp = bus_private.write().unwrap();
                if let Some((snd, start_time)) = bp.ping_data.remove(&peer_id) {
                    let _ = snd.send(start_time.elapsed());
                }
            },
        }
    }

    /// Handle commands coming from the application
    /// 
    /// The application interact with the Ivy thread with [Command]
    fn handle_command(cmd: Command, bus_private: &Arc<RwLock<IvyPrivate>>) {
        let mut bp = bus_private.write().unwrap();
        match cmd {
            Command::Sub(sub_id, regex) => {
                for peer in bp.peers.values() {
                    let msg = IvyMsg::Sub(sub_id, regex.clone());
                    let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                }
            },
            Command::SendMsg(message) => {
                for peer in bp.peers.values() {
                    peer.subscriptions.iter().for_each(|(sub_id, regex)| {
                        // TODO do not recreate the regex each time
                        let re = Regex::new(regex).unwrap();
                        if let Some(caps) = re.captures(&message) {
                            let params = caps.iter()
                                .skip(1)
                                .filter_map(identity)
                                .map(|c| c.as_str().to_string())
                                .collect::<Vec<_>>();
                            let msg = IvyMsg::TextMsg(*sub_id, params);
                            let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                        }
                    })
                }
            },
            Command::SendDirectMsg(peer_id, id, msg) => {
                if let Some(peer) = bp.peers.get(&peer_id) {
                    let msg = IvyMsg::DirectMsg(id, msg);
                    let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                }
            },
            Command::UnSub(sub_id) => {
                for peer in bp.peers.values() {
                    let msg = IvyMsg::DelSub(sub_id);
                    let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                }
            },
            Command::Ping(peer_id, ping_id, snd) => {
                if bp.peers.contains_key(&peer_id) {
                    bp.ping_data.insert(peer_id, (snd, Instant::now()));
                    if let Some(peer) = bp.peers.get(&peer_id) {
                        let msg = IvyMsg::Ping(ping_id);
                        let _ = peer.stream.write().unwrap().write(&msg.to_ascii());
                    }
                }
            },
            Command::Stop => {
                // say Bye to all threads
                for peer in bp.peers.values() {
                    let buf = IvyMsg::Bye.to_ascii();
                    let _ = peer.stream.write().unwrap().write(&buf);
                }

                // Ask all peer threads to terminate
                for peer in bp.peers.values() {
                    peer.should_terminate.store(true, Ordering::Release);
                }

                // TODO try trigger TCPListener

                // remember we have to terminate
                bp.should_terminate.store(true, Ordering::Release);
            }
        }
    }

    /// Listen for incoming TCP connections:
    /// - Report new connections to Ivy thread via the `sen_tcp` channel
    /// - Report terminaison via the `snd_thd_terminated` channel
    fn tcp_listener(listener: TcpListener, sen_tcp: Sender<(TcpStream, SocketAddr)>, term: Arc<AtomicBool>,
                    snd_thd_terminated: Sender<u32>) {
        loop
        {
            match listener.accept() {
                Ok((tcp_stream, addr)) => {
                    if term.load(Ordering::Acquire) { break; }
                    let _ = sen_tcp.send((tcp_stream, addr));
                },
                Err(_error) => {},
            }
        }
        // FIXME this never actually happen because listener.accept is blocking, without timeout...
        let _ = snd_thd_terminated.send(TCPLISTENER_THD_ID);
    }

    fn handle_udp(bus_private: &Arc<RwLock<IvyPrivate>>, addr: SocketAddr, peer_name: String,
            sen_peer: &Sender<(u32, IvyMsg)>, snd_thd_terminated: &Sender<u32>) {
        if let Ok(mut stream) = TcpStream::connect(addr) {
            let stream_tcp_read = stream.try_clone().unwrap();
            let stream_peer = stream.try_clone().unwrap();
            let mut bp = bus_private.write().unwrap();

            let mut peer = Peer::new(stream_peer);
            peer.name = peer_name;
            let peer_id = bp.next_thread_id.fetch_add(1);

            let pd = PeerData {
                socket: stream_tcp_read,
                snd_ivymsg: sen_peer.clone(),
                peer_id,
                term: peer.should_terminate.clone(),
                snd_thd_terminated: snd_thd_terminated.clone(),
            };

            let handle = thread::spawn(move || tcp_read(pd));
            bp.join_handles.insert(peer_id, handle);
            bp.peers.insert(peer_id, peer);
            
            let msg = IvyMsg::PeerId(bp.local_port, bp.appname.clone());
            let buf = msg.to_ascii();
            let _ = stream.write(&buf);
            Self::send_initial_subscriptions(&mut stream, &bp.subscriptions);
        }
    }
}




impl fmt::Debug for IvyBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = self.private.read().unwrap();
        f.debug_struct("IvyBus")
            .field("appname", &p.appname)
            //.field("private", &format_args!("{:?}", *p))
            .finish()
    }
}


impl fmt::Debug for IvyPrivate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("IvyPrivate")
        .field("peers", &self.peers)
        .finish()
    }
}
