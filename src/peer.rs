use std::collections::HashMap;
use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::Ordering;
use std::sync::{atomic::AtomicBool, Arc, RwLock};
use std::time::Duration;
use crossbeam::channel::Sender;

use crate::ivy_messages::IvyMsg;

#[derive(Debug)]
pub struct Peer {
    pub name: String,
    pub id: u32,
    pub subscriptions: HashMap<u32, String>,
    pub stream: RwLock<TcpStream>,
    pub should_terminate: Arc<AtomicBool>,
}

impl Peer {
    pub fn new(stream: TcpStream, id: u32) -> Self {
        Peer {
            name: String::new(),
            id,
            subscriptions: HashMap::new(),
            stream: RwLock::new(stream),
            should_terminate: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }
}

pub struct PeerData {
    pub socket: TcpStream,
    pub snd_ivymsg: Sender<(u32, IvyMsg)>,
    pub peer_id: u32,
    pub term: Arc<AtomicBool>,
    pub snd_thd_terminated: Sender<u32>,
}


/// Receive and parse messages coming from a Peer.
/// 
/// Report received message to Ivy thread via the `data.snd_ivymsg` channel
/// 
/// Report terminaison via the `data.snd_thd_terminated` channel
pub fn tcp_read(mut data: PeerData) {
    let mut buf = vec![0; 1024];
    let _ = data.socket.set_read_timeout(Some(Duration::from_millis(10)));
    loop {
        match data.socket.read(&mut buf) {
            Ok(n) => {
                // socket closed
                if n == 0 { break; }

                let lines = buf[0..n]
                .split(|c| *c==b'\n')
                .filter(|buf| buf.len() > 0);
                for line in lines {
                    if let Ok(msg) = IvyMsg::parse(line) {
                        let _ = data.snd_ivymsg.send((data.peer_id, msg));
                    }
                }
            },
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::WouldBlock => {},
                    _ => {
                        println!("tcp error: {}", e.kind());
                        break;
                    },
                }
            },
        }

        if data.term.load(Ordering::Acquire) {
            break;
        }
    }
    let _ = data.snd_thd_terminated.send(data.peer_id);
}
