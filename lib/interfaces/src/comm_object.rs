use heapless::Vec;
use serde::{Deserialize, Serialize};

pub const MAX_PACKET_SIZE: usize = 256;

pub type CommBytes = [u8; MAX_PACKET_SIZE];

#[derive(Serialize, Deserialize, Debug)]
pub enum CommObject<'a> {
    Text(&'a str),
    Err(&'a str),
}

pub fn serialize_test() -> Result<Vec<u8, MAX_PACKET_SIZE>, postcard::Error> {
    let test3 = CommObject::Text("Hallo, world");
    let test4: Result<Vec<u8, MAX_PACKET_SIZE>, postcard::Error> = postcard::to_vec(&test3);

    test4
}

pub fn deserialize_test<'a>(
    data: &'a Vec<u8, MAX_PACKET_SIZE>,
) -> Result<CommObject<'a>, postcard::Error> {
    let test: Result<CommObject, postcard::Error> = postcard::from_bytes(data);
    test
}
