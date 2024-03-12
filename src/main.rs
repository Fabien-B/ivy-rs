use ivy_rs::IvyBus;
use std::time::Duration;

fn main() {
    let mut bus = IvyBus::new("test");
    let _ = bus.start_ivy_loop("127.255.255.255:2010");
    
    // match bus.start("127.255.255.255:1234").await {
    //     Ok(_) => (),
    //     Err(e) => println!("{:?}", e),
    // }

    std::thread::sleep(Duration::from_secs(1));

    bus.inspect();

    std::thread::sleep(Duration::from_secs(2));
    // bus.stop().await;

    // println!("start ended");
    // Ok(())


}

