use maolan_engine::init;

fn main() {
    println!("before init");
    let client = init();
    println!("before add");
    client.add();
    println!("before play");
    client.play();
    println!("before quit");
    client.quit();
    println!("end");
}
