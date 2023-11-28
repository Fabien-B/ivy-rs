use ivy_rs::IvyBus;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut bus = IvyBus::new("test");
    
    match bus.start("127.255.255.255:2010").await {
        Ok(_) => (),
        Err(e) => println!("{:?}", e),
    }

    std::thread::sleep(Duration::from_secs(5));

    Ok(())


}



// fn main() {
//     std::thread::sleep(Duration::from_secs(3));
//     bus.send_msg("test");
//     std::thread::sleep(Duration::from_secs(5));

//     bus.join();

//     println!("Hello, world!");
// }
