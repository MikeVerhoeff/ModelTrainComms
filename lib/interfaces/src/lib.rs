#![no_std]

mod comm_object;

use heapless::{String};

pub use crate::comm_object::*;

pub fn add(left: u64, right: u64) -> u64 {
    let _test = "hallo";
    let _test2: String<4> = String::new();
    left + right
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
