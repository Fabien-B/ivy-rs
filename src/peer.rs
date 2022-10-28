
use std::{net::TcpStream, io::{Error, Read, Write}};
use std::sync::mpsc;
use std::time::Duration;
use crate::ivy_messages::IvyMsg;


#[derive(Debug)]
pub enum Command {
    Sub(u32, String),
    Msg(String),
    DirectMsg(String),
    Quit,
    Stop,
}

pub struct Peer {
    joinHandle: std::thread::JoinHandle<()>,
    ch_cmd: mpsc::Sender<Command>,
    ch_msg: mpsc::Receiver<Command>,
    //subscriptions: Vec<String>
    //stream: TcpStream,
}

impl Peer {
    pub fn handle_incoming(stream: TcpStream) -> Self {
        // cmd: from other threads to this one
        let (tx_cmd, rx_cmd) = mpsc::channel::<Command>();
        // msg: from this thread to main thread
        let (tx_msg, rx_msg) = mpsc::channel::<Command>();


        
        let mut stream = stream;
        println!("ok: {:?}", stream);
        stream.set_read_timeout(Duration::from_millis(500).into()).unwrap();
        
        
        let th = std::thread::spawn(move || {
            let mut buf = [0; 100];
            loop {
                if let Ok(n) = stream.read(&mut buf) {
                    buf[0..n].split(|c| *c == '\n' as u8)
                    .filter(|b| b.len() != 0)
                    .for_each(|b| {
                        let a = std::str::from_utf8(&b).unwrap();
                        println!("{:?}", a);
                        let ivy_msg = IvyMsg::parse(b);
                        
                    });

                }
                if let Ok(cmd) = rx_cmd.try_recv() {
                    println!("cmd: {:?}", cmd);
                        Peer::send(&mut stream, IvyMsg::EndSub);
                }
            }
        });

        Self {
            joinHandle: th,
            ch_cmd: tx_cmd,
            ch_msg: rx_msg
        }
    }

    fn send(stream: &mut TcpStream, ivymsg: IvyMsg) {
        let buf = ivymsg.to_ascii();
        let b = buf.as_slice();
        stream.write(b);
    }

    pub fn send_message(&self, msg: &str) {
        println!("{}", msg);
        let cmd = Command::Msg(msg.into());
        let _ = self.ch_cmd.send(cmd);
    }

    pub fn send_direct_message(&self, msg: &str) {
        println!("{}", msg);
        let cmd = Command::DirectMsg(msg.into());
        let _ = self.ch_cmd.send(cmd);
    }

}
