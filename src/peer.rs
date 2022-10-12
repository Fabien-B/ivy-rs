
use std::{net::TcpStream, io::{Error, Read}};

pub struct Peer {

}

impl Peer {
    pub fn handle_incoming(stream: TcpStream) {
        let mut stream = stream;
        println!("ok: {:?}", stream);
        //stream.set_read_timeout(Duration::from_millis(100).into());
        
        
        let th = std::thread::spawn(move || {
            let mut buf = [0; 100];
            match stream.read(&mut buf) {
                Ok(n) => println!("{:?}", std::str::from_utf8(&buf[0..n])),
                Err(_) => todo!(),
            }
        });

        th.join();
        
        
        
        
        // let mut buf = [0; 100];

        
        // match stream.read(&mut buf) {
        //     Ok(n) => println!("{:?}", std::str::from_utf8(&buf[0..n])),
        //     Err(_) => todo!(),
        // }
    }
}