use std::io::prelude::*;
use std::fs::{self, File};
use std::path::Path;
use std::sync::Arc;

use chrono::prelude::*;
use td_rthreadpool::ReentrantMutex;

use {FileUtils, TimeUtils};

const KEEP_ROLE_SECOND: u64 = 60 * 60 * 24;

pub const LOG_ERROR: u8 = 1;
pub const LOG_WARN:  u8 = 2;
pub const LOG_INFO:  u8 = 3;
pub const LOG_DEBUG: u8 = 4;
pub const LOG_TRACE: u8 = 5;

pub struct LogUtils {
    file: Option<File>,
    mutex: Arc<ReentrantMutex<i32>>,
    log_path: String,
    basename: String,
    server_id: String,
    roll_size: usize,
    flush_interval: u64,
    check_every_n: u64,
    force_flush_interval: u64,

    cur_file_size: usize,
    cur_count: u64,
    last_append: u64,
    last_flush: u64,
    start_of_period: u64,

    cache_time: u64,
    date_cache: String,
}

static mut EL: *mut LogUtils = 0 as *mut _;
impl LogUtils {
    pub fn instance() -> &'static mut LogUtils {
        unsafe {
            if EL == 0 as *mut _ {
                EL = Box::into_raw(Box::new(LogUtils::new()));
            }
            &mut *EL
        }
    }

    pub fn new() -> LogUtils {
        LogUtils {
            file: None,
            mutex: Arc::new(ReentrantMutex::new(0)),
            log_path: String::new(),
            basename: "tunm".to_string(),
            server_id: "0".to_string(),
            roll_size: 1024 * 1024 * 50,
            flush_interval: 3,
            check_every_n: 1024,
            force_flush_interval: 180,
            cur_file_size: 0,
            cur_count: 0,
            last_append: 0,
            last_flush: 0,
            start_of_period: 0,

            cache_time: 0,
            date_cache: String::new(),

        }
    }

    pub fn set_log_path(&mut self, path: String) {
        self.log_path = path;
        let _ = fs::create_dir_all(Path::new(&*self.log_path));

        if self.file.is_none() {
            self.file = self.role_file();
        }
    }

    pub fn role_file(&mut self) -> Option<File> {
        let filename = self.get_log_filename();
        let file = unwrap_or!(File::create(filename).ok(), return None);
        let now = TimeUtils::get_time_s() as u64;
        let start = now / KEEP_ROLE_SECOND;
        self.cur_file_size = 0;
        self.start_of_period = start;
        self.last_flush = now;
        self.last_append = now;
        self.cur_count = 0;
        Some(file)
    }

    pub fn get_can_use_filename(&mut self, filename: String) -> String {
        if !FileUtils::is_file_exists(&*filename) {
            return filename;
        }

        for i in 1.. {
            let name = format!("{}.{}", filename, i);
            if !FileUtils::is_file_exists(&*name) {
                return name;
            }
        }
        unreachable!("find no use file");
    }

    pub fn set_server_id(&mut self, server_id: String) {
        self.server_id = server_id;
    }

    pub fn get_log_filename(&mut self) -> String {
        
        let tm = Local::now();
        let name = format!("{:4}-{:02}-{:02}_{:02}:{:02}:{:02}",
                                  tm.year() + 1900,
                                  tm.month() + 1,
                                  tm.day(),
                                  tm.hour(),
                                  tm.minute(),
                                  tm.second());
        let filename = self.log_path.clone() + &*self.basename + "_" + &*self.server_id + "_" + &*name + ".log";
        self.get_can_use_filename(filename)
    }

    pub fn append(&mut self, method : u8, log: &str) {
        let mutex = self.mutex.clone();
        let _guard = mutex.lock().unwrap();
        self.append_unlock(method, log);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        if self.file.is_none() {
            return;
        }
        let _ = self.file.as_ref().unwrap().write(bytes);
        self.cur_file_size += bytes.len();
    }

    pub fn write_date(&mut self) {
        if self.file.is_none() {
            return;
        }
        let _ = self.file.as_ref().unwrap().write(self.date_cache.as_bytes());
        self.cur_file_size += self.date_cache.as_bytes().len();
    }

    pub fn write_log_method(&mut self, method : u8) {
        let ret = match method {
            LOG_ERROR => "[error] ",
            LOG_WARN  => "[warn!] ",
            LOG_INFO  => "[info!] ",
            LOG_DEBUG => "[debug] ",
            LOG_TRACE | _ => "[trace] ",
        };
        let _ = self.file.as_ref().unwrap().write(ret.as_bytes());
        self.cur_file_size += ret.as_bytes().len();
    }

    pub fn append_unlock(&mut self, method : u8, log: &str) {
        let mutex = self.mutex.clone();
        let _guard = mutex.lock().unwrap();
        if self.file.is_none() {
            return;
        }
        let now = TimeUtils::get_time_ms() as u64;
        if now != self.cache_time {
            self.cache_time = now;
            let tm = Local::now();
            self.date_cache = format!("[{:4}-{:02}-{:02} {:02}:{:02}:{:02}] ",
                                      tm.year() + 1900,
                                      tm.month() + 1,
                                      tm.day(),
                                      tm.hour(),
                                      tm.minute(),
                                      tm.second());
        }
        self.write_date();
        self.write_log_method(method);
        self.write_bytes(log.as_bytes());
        self.write_bytes(b"\r\n");
        self.check_file_status();
    }

    pub fn check_file_status(&mut self) {
        let now = TimeUtils::get_time_ms() as u64;
        if self.cur_file_size > self.roll_size {
            self.file = self.role_file();
        } else {
            self.cur_count += 1;
            if self.cur_count > self.check_every_n {
                let this_period = now / KEEP_ROLE_SECOND;
                if this_period != self.start_of_period {
                    self.file = self.role_file();
                } else if now - self.last_flush > self.flush_interval {
                    self.last_flush = now;
                    let _ = self.file.as_ref().unwrap().flush();
                }
            } else if now - self.last_flush > self.force_flush_interval {
                self.cur_count = 0;
                self.last_flush = now;
                let _ = self.file.as_ref().unwrap().flush();
            }
        }
    }
}
