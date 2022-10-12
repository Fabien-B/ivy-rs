use ivy_rs::IvyBus;



fn main() {
    let mut bus = IvyBus::new("test");

    match bus.start("127.255.255.255:2010") {
        Ok(_) => println!("Ok!"),
        Err(e) => println!("{:?}", e),
    }

    println!("Hello, world!");
}
