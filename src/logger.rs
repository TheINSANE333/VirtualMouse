// Zero-overhead logger: file logging disabled to maximize throughput and save system resources

#[allow(dead_code)]
pub struct Logger;

#[allow(dead_code)]
impl Logger {
    pub fn new(_path: &str) -> Self {
        Self
    }

    #[inline(always)]
    pub fn write_entry(&self, _tag: &str, _msg: &str) {}

    #[inline(always)]
    pub fn log(&self, tag: &str, msg: &str) {
        println!("[{}] {}", tag, msg);
    }

    #[inline(always)]
    pub fn log_file_only(&self, _tag: &str, _msg: &str) {}
}
