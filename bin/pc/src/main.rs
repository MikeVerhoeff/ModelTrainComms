use std::{error::Error, ffi::CStr};

use interfaces::CommObject;
use serial2_tokio::SerialPort;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");

    let ports = SerialPort::available_ports()?;

    if ports.len() == 0 {
        panic!("No serial ports")
    }
    if ports.len() > 1 {
        println!("Serial Ports: {ports:?}");
        panic!("Can't not pick serial port");
    }

    let port = SerialPort::open(&ports[0], 9600)?;
    port.set_dtr(true)?;

    let (mut receiver, mut sender) = tokio::io::split(port);

    let _ = tokio::join!(serial_reader(receiver), serial_writer(sender));

    Ok(())
}

async fn serial_reader(mut receiver: ReadHalf<SerialPort>) -> Result<(), Box<dyn Error>> {
    loop {
        let mut count = 0;

        let mut buffer = [0u8; 256];
        while count < buffer.len() {
            let bytes = receiver.read(&mut buffer[count..]).await?;
            count += bytes;
        }
        let test: Result<CommObject, postcard::Error> = postcard::from_bytes(&buffer);
        println!(
            "{}: {test:?}",
            chrono::Local::now().format("[%H:%M.%S%.6f] ")
        );
    }
}

async fn serial_writer(mut writer: WriteHalf<SerialPort>) -> Result<(), Box<dyn Error>> {
    println!("Writer started");
    loop {
        let mut input_buffer = [0u8; 256];
        let result_size = tokio::io::stdin().read(&mut input_buffer).await?;
        let cstr = CStr::from_bytes_until_nul(&input_buffer[..result_size + 1])?;
        let str = cstr.to_str()?;
        println!("Got string: '{str}'");

        let message = CommObject::Text(str);
        let mut message_buffer = [0u8; 256];
        match postcard::to_slice(&message, &mut message_buffer) {
            Ok(_) => match writer.write_all(&message_buffer).await {
                Ok(_) => println!("Send: {message:?}"),
                Err(e) => println!("Serial Error: {e}"),
            },
            Err(e) => println!("Postcard Error: {e}"),
        }
    }
}

#[test]
fn serialization_test() {
    let serialize_results = interfaces::serialize_test();
    println!("Serialize result: {:?}", serialize_results);
    if let Ok(bytes) = serialize_results {
        let deserialize_result = interfaces::deserialize_test(&bytes);
        println!("Deserialize result: {:?}", deserialize_result);
    }
}
