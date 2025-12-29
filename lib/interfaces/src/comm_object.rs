use heapless::{String, Vec};
use serde::{Deserialize, Serialize};

pub const MAX_PACKET_SIZE: usize = 256;

pub type CommBytes = [u8; MAX_PACKET_SIZE];

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CommObject {
    Text(String<128>),
    Err(String<128>),
    Samples(Vec<u16, 64>),
}

pub fn serialize_test() -> Result<Vec<u8, MAX_PACKET_SIZE>, postcard::Error> {
    let test3 = CommObject::Text("Hallo, world".into());
    let test4: Result<Vec<u8, MAX_PACKET_SIZE>, postcard::Error> = postcard::to_vec(&test3);

    test4
}

pub fn deserialize_test(data: &Vec<u8, MAX_PACKET_SIZE>) -> Result<CommObject, postcard::Error> {
    let test: Result<CommObject, postcard::Error> = postcard::from_bytes(data);
    test
}
