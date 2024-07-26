pub fn sha256<Data: AsRef<[u8]>>(data: Data) -> String {
    use sha2::Digest;
    format!("{:x}", sha2::Sha256::digest(data))
}
