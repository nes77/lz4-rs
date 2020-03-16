#![forbid(unsafe_code)]
#![deny(missing_docs)]

//! An implementation of the [LZ4 algorithm]
//! [LZ4 algorithm]: https://github.com/lz4/lz4/wiki

pub mod lz77;
mod matchmap;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
