use std::error::Error;

use serial2_tokio::SerialPort;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");

    let ports = SerialPort::available_ports()?;

    if ports.len()==0 {
        panic!("No serial ports")
    }
    if ports.len()>1 {
        println!("Serial Ports: {ports:?}");
        panic!("Can't not pick serial port");
    }

    let port = SerialPort::open(&ports[0], 9600)?;

    let send_buf = b"test";
    port.write(send_buf).await?;

    let mut count = 0;

    while count<17 {
        let mut buffer = [0u8; 256];
        let bytes = port.read(&mut buffer).await?;
        //print!("[{bytes}:{:?}]", &buffer[0..bytes]);
        for c in &buffer[0..bytes] {
            print!("{}", *c as char);
        }
        count += bytes;
    }

    Ok(())
}
