use interfaces::CommObject;
use interfaces::encoding;

#[test]
fn it_works() {
    let object = CommObject::Text("Hallo, world");
    let mut buf = [0u8; 256];
    match postcard::to_slice(&object, &mut buf) {
        Ok(buf) => {
            println!("{}", buf.len())
        }
        Err(e) => println!("{}", e),
    }
    println!("{:?}", object);
}

#[test]
fn test_encoding() {
    const DATA_SIZE: usize = 16;
    const PREAMBLE_SIZE: usize = 2;
    let mut buffer = [0u8; PREAMBLE_SIZE + 2 * DATA_SIZE];
    let data = [1u8; DATA_SIZE];
    let preamble = [0b10101011u8, 0b10001111];
    assert_eq!(preamble.len(), PREAMBLE_SIZE);

    buffer[0..PREAMBLE_SIZE].copy_from_slice(&preamble);
    buffer[PREAMBLE_SIZE..PREAMBLE_SIZE + DATA_SIZE].copy_from_slice(&data);

    println!("{:?}", buffer);
}

#[test]
fn test_encoder() {
    let mut encoder = encoding::Encoder::new();
    let mut data = [1u8; encoding::DATA_SIZE];
    for i in 0..data.len() {
        data[i] = (i * i) as u8;
    }
    encoder.get_data_slice().copy_from_slice(&data);
    let encoded_data = encoder.encode();
    print!("Encoded: ");
    for byte in encoded_data {
        print!("{byte:08b} ");
    }
    println!();
    println!();
    print!("Decoded: ");
    let decoded_data = encoder.decode();
    for byte in decoded_data {
        print!("{byte} ");
    }
    println!();
}

#[test]
fn test_input_encoder() {
    let mut encoder = encoding::Encoder::new();
    let mut data = [1u8; encoding::DATA_SIZE];
    for i in 0..data.len() {
        data[i] = (i * i) as u8;
    }
    encoder.get_data_slice().copy_from_slice(&data);
    let encoded_data = encoder.encode();
    print!("Encoded: ");
    for byte in encoded_data {
        print!("{byte:08b} ");
    }

    let mut decoder = encoding::Encoder::new();
    println!(
        "{:?}",
        decoder.encoded_input(&mut [0b01011001u8, 0b10100101u8, 0b10011010], 3)
    );
    println!("{:?}", decoder.encoded_input(&encoded_data[0..10], 10));
    println!("{:?}", decoder.encoded_input(&encoded_data[10..], 10));
    println!(
        "{:?}",
        decoder.encoded_input(&encoded_data[20..], encoded_data.len() - 20)
    );
    println!("{:?}", decoder.decode());
}
