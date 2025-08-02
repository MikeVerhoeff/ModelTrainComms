use heapless::Vec;
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Debug)]
pub enum CommObject<'a> {
    Text(&'a str),
    Err(&'a str),
}

pub fn serialize_test() -> Result<Vec<u8, 128>, postcard::Error> {
    
    let test3 = CommObject::Text("Hallo, world");
    let test4: Result<Vec<u8, 128>, postcard::Error> = postcard::to_vec(&test3);

    return test4;
}

pub fn deserialize_test<'a>(data: &'a Vec<u8, 128>) -> Result<CommObject<'a>, postcard::Error> {
    let test: Result<CommObject, postcard::Error> = postcard::from_bytes(data);
    return test;
}