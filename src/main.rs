use ivy_rs::IvyBus;
use std::time::Duration;

fn main() {
    let mut bus = IvyBus::new("test");
    bus.set_client_connected_cb(Box::new(|| println!("plop")));
    bus.subscribe("(az.*)", Box::new(|params| println!("cb AZ args:{params:?}")));

    let _ = bus.start_ivy_loop("127.255.255.255:2010");
    std::thread::sleep(Duration::from_secs(1));


    bus.subscribe("(yo.*)", Box::new(|params| println!("cb YO args:{params:?}")));
    std::thread::sleep(Duration::from_secs(1));


    bus.send("hello you");
    std::thread::sleep(Duration::from_secs(1));

    bus.stop();
    println!("stopped!");

    std::thread::sleep(Duration::from_secs(20));
    // bus.stop().await;

    // println!("start ended");
    // Ok(())


}

