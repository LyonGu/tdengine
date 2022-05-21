use tunm_proto::Buffer;
use psocket::{SOCKET};
use mio::{Poll, Token};
use mio::net::{TcpListener, TcpStream};
use crate::net::AsSocket;


pub type AcceptCb = fn(ev: &mut Poll, &mut SocketEvent) -> usize;
pub type ReadCb = fn(ev: &mut Poll, &mut SocketEvent) -> usize;
pub type WriteCb = fn(ev: &mut Poll, &mut SocketEvent) -> usize;
pub type EndCb = fn(ev: &mut Poll, &mut SocketEvent);

// #[derive(Debug)]
pub struct SocketEvent {
    socket_fd: SOCKET,
    cookie: u32,
    client_ip: String,
    server_port: u16,
    pub in_buffer: Buffer,
    pub out_buffer: Buffer,
    online: bool,
    websocket: bool,
    local: bool, //is local create fd
    mio: bool,
    server: Option<TcpListener>,
    client: Option<TcpStream>,
    accept: Option<AcceptCb>,
    read: Option<ReadCb>,
    write: Option<WriteCb>,
    end: Option<EndCb>,
}

impl SocketEvent {
    pub fn new(socket_fd: SOCKET, client_ip: String, server_port: u16) -> SocketEvent {
        SocketEvent {
            socket_fd: socket_fd,
            cookie: 0,
            client_ip: client_ip,
            server_port: server_port,
            in_buffer: Buffer::new(),
            out_buffer: Buffer::new(),
            online: true,
            websocket: false,
            local: false,
            mio: false,
            server: None,
            client: None,
            accept: None,
            read: None,
            write: None,
            end: None,
        }
    }
    
    pub fn new_client(client: TcpStream, server_port: u16) -> SocketEvent {
        let peer = format!("{}", client.peer_addr().unwrap());
        SocketEvent {
            socket_fd: client.as_socket() as SOCKET,
            cookie: 0,
            client_ip: peer,
            server_port: server_port,
            in_buffer: Buffer::new(),
            out_buffer: Buffer::new(),
            online: true,
            websocket: false,
            local: false,
            mio: false,
            server: None,
            client: Some(client),
            accept: None,
            read: None,
            write: None,
            end: None,
        }
    }
    
    pub fn new_server(server: TcpListener, server_port: u16) -> SocketEvent {
        SocketEvent {
            socket_fd: server.as_socket() as SOCKET,
            cookie: 0,
            client_ip: "".to_string(),
            server_port: server_port,
            in_buffer: Buffer::new(),
            out_buffer: Buffer::new(),
            online: true,
            websocket: false,
            local: false,
            mio: false,
            server: Some(server),
            client: None,
            accept: None,
            read: None,
            write: None,
            end: None,
        }
    }

    pub fn get_socket_fd(&self) -> i32 {
        self.socket_fd as i32
    }
    
    pub fn as_raw_socket(&self) -> SOCKET {
        self.socket_fd
    }

    pub fn as_token(&self) -> Token {
        Token(self.socket_fd as usize)
    }

    pub fn get_client_ip(&self) -> String {
        self.client_ip.clone()
    }

    pub fn get_server_port(&self) -> u16 {
        self.server_port
    }

    pub fn get_cookie(&self) -> u32 {
        self.cookie
    }

    pub fn set_cookie(&mut self, cookie: u32) {
        self.cookie = cookie;
    }

    pub fn get_in_buffer(&mut self) -> &mut Buffer {
        &mut self.in_buffer
    }

    pub fn get_out_buffer(&mut self) -> &mut Buffer {
        &mut self.out_buffer
    }

    pub fn set_online(&mut self, online: bool) {
        self.online = online;
    }

    pub fn is_online(&self) -> bool {
        self.online
    }

    pub fn set_websocket(&mut self, websocket: bool) {
        self.websocket = websocket;
    }

    pub fn is_websocket(&self) -> bool {
        self.websocket
    }

    pub fn set_local(&mut self, local: bool) {
        self.local = local;
    }

    pub fn is_local(&self) -> bool {
        self.local
    }

    pub fn set_mio(&mut self, mio: bool) {
        self.mio = mio;
    }

    pub fn is_mio(&self) -> bool {
        self.mio
    }
    
    pub fn set_server(&mut self, server: TcpListener) {
        self.server = Some(server);
    }

    pub fn is_server(&self) -> bool {
        self.server.is_some()
    }
    
    pub fn as_server(&mut self) -> Option<&mut TcpListener> {
        self.server.as_mut()
    }

    
    pub fn set_client(&mut self, client: TcpStream) {
        self.client = Some(client);
    }

    pub fn is_client(&self) -> bool {
        self.client.is_some()
    }
    
    pub fn as_client(&mut self) -> Option<&mut TcpStream> {
        self.client.as_mut()
    }

    pub fn set_accept(&mut self, accept: Option<AcceptCb>) {
        self.accept = accept;
    }

    pub fn call_accept(&self, poll: &mut Poll, client: &mut SocketEvent) -> usize {
        if self.accept.is_some() {
            self.accept.as_ref().unwrap()(poll, client)
        } else {
            0
        }
    }
    
    pub fn set_read(&mut self, read: Option<ReadCb>) {
        self.read = read;
    }
    
    pub fn call_read(&self, poll: &mut Poll, client: &mut SocketEvent) -> usize {
        if self.read.is_some() {
            self.read.as_ref().unwrap()(poll, client)
        } else {
            0
        }
    }

    
    pub fn set_write(&mut self, write: Option<WriteCb>) {
        self.write = write;
    }

    pub fn call_write(&self, poll: &mut Poll, client: &mut SocketEvent) -> usize {
        if self.write.is_some() {
            self.write.as_ref().unwrap()(poll, client)
        } else {
            0
        }
    }
    
    pub fn set_end(&mut self, end: Option<EndCb>) {
        self.end = end;
    }
    
    pub fn call_end(&self, poll: &mut Poll, client: &mut SocketEvent) {
        if self.end.is_some() {
            self.end.as_ref().unwrap()(poll, client);
        }
    }
}
