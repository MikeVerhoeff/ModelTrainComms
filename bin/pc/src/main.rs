mod gui;

use std::{env, error::Error, ffi::CStr};

use interfaces::CommObject;
use serial2_tokio::SerialPort;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    runtime::Runtime,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();
    if args.contains(&String::from("--no-gui")) {
        let rt = Runtime::new()?;
        rt.block_on(async { commandline_main().await })?;
    } else {
        println!("Gui Init");
        gui::run()?;
    }

    Ok(())
}

//#[tokio::main]
async fn commandline_main() -> Result<(), Box<dyn Error>> {
    println!("Init");

    let ports = SerialPort::available_ports()?;

    if ports.len() == 0 {
        panic!("No serial ports")
    }
    if ports.len() > 1 {
        println!("Serial Ports: {ports:?}");
        panic!("Can't not pick serial port");
    }

    let port = SerialPort::open(&ports[0], 921600)?;
    port.set_dtr(true)?;

    let (receiver, sender) = tokio::io::split(port);

    let _ = tokio::join!(serial_reader(receiver), serial_writer(sender));

    Ok(())
}

async fn serial_reader(mut receiver: ReadHalf<SerialPort>) -> Result<(), Box<dyn Error>> {
    println!("Reader started");

    let mut adc_values = [0u16; 4 * 64];
    let mut adc_value_index = 0;

    loop {
        let mut count = 0;

        let mut buffer = [0u8; 256];
        while count < buffer.len() {
            let bytes = receiver.read(&mut buffer[count..]).await?;
            count += bytes;
        }
        let result: Result<CommObject, postcard::Error> = postcard::from_bytes(&buffer);

        if let Ok(CommObject::Samples(values)) = &result {
            adc_values[adc_value_index..adc_value_index + values.len()].copy_from_slice(&values);
            adc_value_index += values.len();
            if adc_values.len() == adc_value_index {
                println!("Buffer full: {adc_values:?}");
                adc_value_index = 0;
            }
        }

        println!(
            "{}: {result:?}",
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

        let message = CommObject::Text(str.into());
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
