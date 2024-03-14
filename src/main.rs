use ivy_rs::IvyBus;
use std::time::Duration;

fn main() {
    let mut bus = IvyBus::new("test");
    bus.set_client_connected_cb(Box::new(|peer| println!("Connected to {}", peer.get_name())));
    bus.subscribe("az ([0-9]+) (.*)", Box::new(|_peer, params| println!("cb AZ args:{params:?}")));

    let _ = bus.start("127.255.255.255:2010");

    std::thread::sleep(Duration::from_secs(1));

    bus.set_direct_message_cb(Box::new(|peer,id,msg| {
        println!("Direct msg from {}: [{id}] {msg}", peer.get_name())
    }));

    bus.set_quit_cb(Box::new(|peer| {
        println!("{} wants to kill me!", peer.get_name())
    }));
    
    bus.subscribe("(yo.*)", Box::new(|_peer, params| println!("cb YO args:{params:?}")));
    //std::thread::sleep(Duration::from_secs(1));


    for _ in 0..40 {
        bus.send("hello you");
        std::thread::sleep(Duration::from_secs(2));
    }
    std::thread::sleep(Duration::from_secs(20));

    bus.stop();
    println!("stopped!");

    std::thread::sleep(Duration::from_secs(1));
    // bus.stop().await;

    // println!("start ended");
    // Ok(())


}

