use std::iter::repeat;
use std::ffi::{CString};
use crypto::aes_gcm::AesGcm;
use crypto::aes::{KeySize};
use crypto::aead::AeadDecryptor;
use {NetMsg, FileUtils};
use td_rlua::{self, Lua, LuaRead};
use libc;
use tunm_proto;
use std::sync::Arc;
use td_rthreadpool::ReentrantMutex;
use td_rredis::{RedisResult, Value};
use super::{LuaWrapperTableValue, RedisWrapperResult};
use psocket::SOCKET;

const AES_KEY: [u8; 32] = [
            0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe,
            0x2b, 0x72, 0xaf, 0xf0, 0x85, 0x7d, 0x77, 0x81,
            0x1f, 0x35, 0x2c, 0x08, 0x3b, 0x62, 0x08, 0xd7,
            0x2d, 0x98, 0x10, 0xa3, 0x09, 0x14, 0xdf, 0xf4 
        ];


const AES_IV: [u8; 12] = [
            0x60, 0x3d, 0xeb, 0x10, 0x15, 0xca, 0x71, 0xbe,
            0x2b, 0x72, 0xaf, 0xf0
        ];

static mut EL: *mut LuaEngine = 0 as *mut _;
/// the type of lua call type
enum LuaElem {
    /// fd, msg
    Message(SOCKET, NetMsg),
    /// cookie, ret, err_msg, msg
    DbResult(u32, i32, Option<String>, Option<NetMsg>),
    /// cookie, value
    RedisResult(u32, Option<RedisResult<Value>>),
    /// cookie, new_fd, client_ip, server_port, websocket
    NewConnection(u32, SOCKET, String, u16, bool),
    /// fd
    LostConnection(SOCKET),
    /// func_str
    ExecString(String),
    /// Args fuc
    ArgsFunc(String, Vec<String>),
}

/// the enterface to call lua, it store the lua state and exec list
pub struct LuaEngine {
    exec_list: Vec<LuaElem>,
    lua: Lua,
    mutex: Arc<ReentrantMutex<i32>>,
}

/// custom lua load func
extern "C" fn load_func(lua: *mut td_rlua::lua_State) -> libc::c_int {
    let path: String = unwrap_or!(td_rlua::LuaRead::lua_read(lua), return 0);
    println!("loading path == {:?}", path);
    let full_path = unwrap_or!(FileUtils::instance().full_path_for_name(&*path), path);
    let full_path = full_path.trim_matches('\"');
    let data = unwrap_or!(FileUtils::get_file_data(&*full_path).ok(), return 0);
    if data.len() < 10 {
        return 0;
    }
    let mut name = full_path.to_string();
    let mut short_name = name.clone();
    let len = name.len();
    if len > 30 {
        short_name = name.drain((len - 30)..).collect();
    }

    let short_name = CString::new(short_name).unwrap();
    let ret;
    if data[0] == 0xff && data[1] == 0xfe && data[2] == 0xfd && data.len() > 19 {
        let mut out: Vec<u8> = repeat(0).take(data.len() - 19).collect();
        let mut decipher = AesGcm::new(KeySize::KeySize256, &AES_KEY, &AES_IV, &[0;0]);
        let _result = decipher.decrypt(&data[19..], &mut out[..], &data[3..19]);
        ret = unsafe { td_rlua::luaL_loadbuffer(lua, out.as_ptr() as *const i8, out.len(), short_name.as_ptr()) };
    } else {
        ret = unsafe { td_rlua::luaL_loadbuffer(lua, data.as_ptr() as *const i8, data.len(), short_name.as_ptr()) };
    }
    if ret != 0 {
        let err_msg : String = unwrap_or!(LuaRead::lua_read(lua), return 0);
        let err_detail = CString::new(format!("error loading from file {} :\n\t{}", full_path, err_msg)).unwrap();
        unsafe { td_rlua::luaL_error(lua, err_detail.as_ptr()); }
    }
    1
}

impl LuaEngine {
    pub fn instance() -> &'static mut LuaEngine {
        unsafe {
            if EL == 0 as *mut _ {
                EL = Box::into_raw(Box::new(LuaEngine::new(Lua::new())));
            }
            &mut *EL
        }
    }

    pub fn new(mut lua: Lua) -> LuaEngine {
        lua.openlibs();
        lua.add_lualoader(load_func);
        lua.enable_hotfix();
        LuaEngine {
            exec_list: vec![],
            lua: lua,
            mutex: Arc::new(ReentrantMutex::new(0)),
        }
    }

    pub fn get_lua(&mut self) -> &mut Lua {
        &mut self.lua
    }

    pub fn execute_lua(&mut self) -> bool {
        let temp_list: Vec<LuaElem>;
        {
            let _guard = self.mutex.lock().unwrap();
            temp_list = self.exec_list.drain(..).collect();
        }
        for elem in temp_list {
            let _ = match elem {
                LuaElem::Message(fd, net_msg) => self.execute_message(fd, net_msg),
                LuaElem::DbResult(cookie, ret, err_msg, net_msg) => {
                    self.execute_db_result(cookie, ret, err_msg, net_msg)
                }
                LuaElem::RedisResult(cookie, result) => self.execute_redis_result(cookie, result),
                LuaElem::NewConnection(cookie, new_fd, client_ip, server_port, websocket) => {
                    self.execute_new_connect(cookie, new_fd, client_ip, server_port, websocket)
                }
                LuaElem::LostConnection(lost_fd) => self.execute_lost_connect(lost_fd),
                LuaElem::ExecString(func_str) => self.execute_string(func_str),
                LuaElem::ArgsFunc(func, args) => self.execute_args_func(func, args),
            };
        }
        true
    }

    pub fn apply_new_connect(&mut self,
                             cookie: u32,
                             new_fd: SOCKET,
                             client_ip: String,
                             server_port: u16,
                             websocket: bool) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::NewConnection(cookie, new_fd, client_ip, server_port, websocket));
    }

    pub fn apply_lost_connect(&mut self, lost_fd: SOCKET) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::LostConnection(lost_fd));
    }

    pub fn apply_db_result(&mut self,
                           cookie: u32,
                           ret: i32,
                           err_msg: Option<String>,
                           net_msg: Option<NetMsg>) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::DbResult(cookie, ret, err_msg, net_msg));
    }

    pub fn apply_redis_result(&mut self, cookie: u32, result: Option<RedisResult<Value>>) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::RedisResult(cookie, result));
    }

    pub fn apply_message(&mut self, fd: SOCKET, net_msg: NetMsg) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::Message(fd, net_msg));
    }

    pub fn apply_exec_string(&mut self, func_str: String) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::ExecString(func_str));
    }


    pub fn apply_args_func(&mut self, func: String, args: Vec<String>) {
        let _guard = self.mutex.lock().unwrap();
        self.exec_list.push(LuaElem::ArgsFunc(func, args));
    }

    pub fn execute_new_connect(&mut self,
                               cookie: u32,
                               new_fd: SOCKET,
                               client_ip: String,
                               server_port: u16,
                               websocket: bool)
                               -> i32 {
        self.lua.exec_func5("cmd_new_connection", cookie, new_fd, client_ip, server_port, websocket)
    }

    pub fn execute_lost_connect(&mut self, lost_fd: SOCKET) -> i32 {
        self.lua.exec_func1("cmd_connection_lost", lost_fd)
    }

    pub fn execute_db_result(&mut self,
                             cookie: u32,
                             ret: i32,
                             err_msg: Option<String>,
                             net_msg: Option<NetMsg>)
                             -> i32 {
        if ret != 0 {
            self.lua.exec_func3("msg_db_result",
                                cookie,
                                ret,
                                err_msg.unwrap_or("err msg detail miss".to_string()))
        } else {
            if net_msg.is_some() {
                // if let Some(net_msg) = net_msg.as_mut() {
                let mut net_msg = net_msg.unwrap();
                net_msg.set_read_data();
                if let Ok((_, val)) = tunm_proto::decode_proto(net_msg.get_buffer()) {
                    self.lua.exec_func3("msg_db_result", cookie, ret, LuaWrapperTableValue(val))
                } else {
                    self.lua.exec_func3("msg_db_result", cookie, -2, "analyse data failed")
                }
            } else {
                self.lua.exec_func3("msg_db_result", cookie, ret, LuaWrapperTableValue(vec![]))
            }
        }
    }

    pub fn execute_redis_result(&mut self, cookie: u32, result: Option<RedisResult<Value>>) -> i32 {
        if result.is_none() {
            self.lua.exec_func1("msg_redis_result", cookie)
        } else {
            self.lua.exec_func2("msg_redis_result",
                                cookie,
                                RedisWrapperResult(result.unwrap()))
        }
    }

    pub fn execute_message(&mut self, fd: SOCKET, mut net_msg: NetMsg) -> i32 {
        net_msg.set_read_data();
        unwrap_or!(net_msg.read_head().ok(), return -1);
        self.lua.exec_func3("global_dispatch_command",
                            fd,
                            net_msg.get_pack_name().clone(),
                            net_msg)
    }

    pub fn execute_string(&mut self, func_str: String) -> i32 {
        self.lua.exec_func1("RUN_STRING", func_str)
    }

    pub fn execute_args_func(&mut self, func: String, args: Vec<String>) -> i32 {
        // println!("execute func is {}", func);
        match args.len() {
            0 => self.lua.exec_func0(func),
            1 => self.lua.exec_func1(func, &*args[0]),
            2 => self.lua.exec_func2(func, &*args[0], &*args[1]),
            3 => self.lua.exec_func3(func, &*args[0], &*args[1], &*args[2]),
            4 => self.lua.exec_func4(func, &*args[0], &*args[1], &*args[2], &*args[3]),
            5 => self.lua.exec_func5(func, &*args[0], &*args[1], &*args[2], &*args[3], &*args[4]),
            6 => {
                self.lua.exec_func6(func,
                                    &*args[0],
                                    &*args[1],
                                    &*args[2],
                                    &*args[3],
                                    &*args[4],
                                    &*args[5])
            }
            7 => {
                self.lua.exec_func7(func,
                                    &*args[0],
                                    &*args[1],
                                    &*args[2],
                                    &*args[3],
                                    &*args[4],
                                    &*args[5],
                                    &*args[6])
            }
            _ => -1,
        }
    }

    pub fn convert_excute_string(mut ori: String) -> String {
        if ori.len() == 0 {
            return ori;
        }

        if let Some(index) = ori.find('\'') {
            if index == 0 {
                let t: String = ori.drain(1..).collect();
                return format!("LOG.warn(\"%o\", {})", t);
            }
        }

        return ori;
    }

    pub fn do_hotfix_file(&mut self, path: String) -> i32 {
        let full_path = unwrap_or!(FileUtils::instance().full_path_for_name(&*path), path);
        let full_path = full_path.trim_matches('\"');
        let data = unwrap_or!(FileUtils::get_file_data(&*full_path).ok(), return 0);
        if data.len() < 10 {
            return 0;
        }
        let mut name = full_path.to_string();
        let mut short_name = name.clone();
        let len = name.len();
        if len > 30 {
            short_name = name.drain((len - 30)..).collect();
        }

        let file_data = if data[0] == 0xff && data[1] == 0xfe && data[2] == 0xfd && data.len() > 19 {
            let mut out: Vec<u8> = repeat(0).take(data.len() - 19).collect();
            let mut decipher = AesGcm::new(KeySize::KeySize256, &AES_KEY, &AES_IV, &[0;0]);
            let _result = decipher.decrypt(&data[19..], &mut out[..], &data[3..19]);
            unwrap_or!(String::from_utf8(out).ok(), return 0)
        } else {
            unwrap_or!(String::from_utf8(data).ok(), return 0)
        };
        self.lua.exec_func2("hotfix", file_data, short_name)
    }
}
