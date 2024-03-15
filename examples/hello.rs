use ivy_rs::{peer::Peer, IvyBus};
use std::{sync::{Arc, Mutex}, time::Duration};

fn main() {
    println!("Creating a new IvyBus \"Hello Rust\"");
    let bus = Arc::new(Mutex::new(IvyBus::new("Hello Rust")));

    // can be done after bus.lock().unwrap() started
    bus.lock().unwrap().set_client_connected_cb(Box::new(
        |peer| println!("New peer \"{}\" has connected", peer.get_name())
    ));

    // can be done after bus.lock().unwrap() started
    bus.lock().unwrap().set_direct_message_cb(Box::new(|peer,id,msg| {
        println!("Direct msg from {}: [{id}] {msg}", peer.get_name())
    }));

    // can be done after bus.lock().unwrap() started
    bus.lock().unwrap().set_quit_cb(Box::new(|peer| {
        println!("{} wants to kill me!", peer.get_name())
    }));

    let cbus = bus.clone();
    let bind_id = Arc::new(Mutex::new(None));
    let bind_id_cb = bind_id.clone();
    let bb = bus.lock().unwrap().subscribe("(once.*)", Box::new(
        move |peer, params| {
            println!("pre-start subscribe: {params:?}");
            cbus.lock().unwrap().send_direct_msg(peer.get_id(), 42, "yo man".into());
            if let Some(id) = bind_id_cb.lock().unwrap().to_owned() {
                cbus.lock().unwrap().unsubscribe(id);
            }
        }
    ));
    *bind_id.lock().unwrap() = Some(bb);


    let _ = bus.lock().unwrap().start("127.255.255.255:2010");
    
    std::thread::sleep(Duration::from_secs(1));

    bus.lock().unwrap().subscribe("(yo.*)", Box::new(yo_cb));

    // let p = bus.lock().unwrap().ping(2, Duration::from_millis(500));
    // println!("ping: {p:?}");

    // for _ in 0..40 {
    //     bus.lock().unwrap().send("hello you");
    //     std::thread::sleep(Duration::from_secs(2));
    // }
    std::thread::sleep(Duration::from_secs(2));

    println!("try stop");
    bus.lock().unwrap().stop();
    println!("stopped!");

    std::thread::sleep(Duration::from_secs(1));



}


fn yo_cb(_peer: &Peer, params: Vec<String>) {
    println!("Regular function: {params:?}");
}