#![feature(unix_socket_ancillary_data)]
use std::io;
use std::os::fd::AsRawFd;
use chrono::{Local};
use std::path::Path;
use std::fs;
use std::time::{Duration,Instant};
extern crate libnotify;
use std::io::{Write,Read};
use std::thread;
use std::os::unix::net::{UnixListener, SocketAncillary};
use nix::unistd::gethostname;
use std::net::{TcpListener, TcpStream, SocketAddr};

pub struct Connection {
	stream: TcpStream,
	address: SocketAddr,
	message_buffer: String,
	name: String,
}

const SOCKET_LOCATION: &str = "/tmp/vanillachatd.socket";

fn main() -> io::Result<()>{
	let mut connections: Vec<Connection> = vec![];
	let our_name: String = gethostname()?.into_string().unwrap_or("Unknown name".into());
	//===== setup the listener ======
	let port: u16 = 9567;
	let addr = SocketAddr::from(([0,0,0,0],port));
	let listener = TcpListener::bind(addr)?;
	if Path::new(SOCKET_LOCATION).exists(){
		//delete socket if it exists
		fs::remove_file(SOCKET_LOCATION)?;
	}
	let ipc = UnixListener::bind(SOCKET_LOCATION)?;
	ipc.set_nonblocking(true)?;
	//nonblocking
	listener.set_nonblocking(true).expect("could not set listener to nonblocking");
	loop{
		//====== accept tcp connections ======
		let _ = match listener.accept(){
			Ok(connection) => handle_connection(&mut connections,connection.0,connection.1,&our_name),
			Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
			Err(e) => panic!("Error: {e}"),
		};
		//====== accept ipc connections ======
		let _ = handle_ipc(&ipc,&mut connections);
		//====== receive any messages ======
		for i in 0..connections.len(){
			let message = recv_msg(connections.get_mut(i).unwrap(),None);
			if message.is_some(){
				let _ = send_notification(&connections[i],message.unwrap());
			}
		}
		//====== verify sockets are still alive ======
		let mut connections_to_delete = vec![];
		for connection in connections.iter().enumerate(){
			if !is_alive(connection.1){
				println!("connection {} dead",connection.1.address);
				connections_to_delete.push(connection.0);
			}
		}
		for connection_to_delete in connections_to_delete{
			connections.swap_remove(connection_to_delete);
		}

		//====== yield cpu time to other processes ======
		thread::sleep(Duration::from_millis(20));
	}
}
fn handle_connection(connections: &mut Vec<Connection>, stream: TcpStream, address: SocketAddr, our_name: &String) -> Result<(), io::Error>{
	println!("New connection: {}",address);
	let mut connection = Connection {
		stream: stream,
		address: address,
		message_buffer: "".to_string(),
		name: String::new()
	};
	//====== send our name ======
	send_msg(&mut connection.stream,our_name.clone())?;
	//======= give the client 5s to send their name ======
	let timeout = Duration::from_secs(5);
	let name = recv_msg(&mut connection,Some(timeout)).unwrap_or("name unknown".into());
	connection.name = name;
	connections.push(connection);
	Ok(())
}
fn is_alive(connection: &Connection) -> bool{
	connection.stream.set_nonblocking(true).expect("could not place connection socket into nonblocking mode");
	let mut buf = [0; 1];
	let result = connection.stream.peek(&mut buf);
	connection.stream.set_nonblocking(false).expect("could not place connection socket into blocking mode");
	match result {
		Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => true,
		Ok(size) => match size {
			0 => false,
			_ => true,
		},
		Err(_) => false,
	}
}
fn recv_msg(connection: &mut Connection,timeout: Option<Duration>) -> Option<String>{
	//switch to nonblocking
	connection.stream.set_nonblocking(true).expect("could not place connection socket into nonblocking mode");
	//start the timer
	let stopwatch = Instant::now();
	//====== read ======
	let mut buffer = [0; 1];
	let return_value = loop{
		//exit if timeout reached
		if timeout.is_some(){
			if stopwatch.elapsed() >= timeout.unwrap(){
				break None;
			}
		}
		let count = match connection.stream.read(&mut buffer){
			Ok(count) => count,
			Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => 0,
			Err(e) => {eprintln!("error whilst reading: {e}");break None},
		};
		if count == 1 {
			if buffer[0] == 0x04{
				//end of transmition
				let ret = connection.message_buffer.clone();
				connection.message_buffer.clear();
				break Some(ret);
			}
			//====== append the char to the buffer ======
			let ch = char::from_u32(buffer[0] as u32).unwrap();
			connection.message_buffer.push(ch);
		}else{
			break None;
		}
	};
	//unswitch from nonblocking
	connection.stream.set_nonblocking(false).expect("could not place connection socket into blocking mode");
	return_value
}
fn send_notification(connection: &Connection, message: String) -> Result<(),String>{
	println!("new message: {message}");
	libnotify::init("vanillachatd")?;
	let notification = libnotify::Notification::new(format!("vanillachat @{}",connection.address).as_str(),Some(message.as_str()),None);
	match notification.show(){
		Ok(_) => Ok(()),
		Err(e) => Err(e.to_string()),
	}?;
	libnotify::uninit();
	Ok(())
}
fn handle_ipc(listener: &UnixListener, connections: &mut Vec<Connection>) -> io::Result<()>{
	//====== accept connection ======
	let (mut connection, _addr) = match listener.accept(){
		Ok(c) => c,
		//yeild if no connection ready
		Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(()),
		Err(e) => return Err(e),
	};
	println!("new ipc connection");
	//====== send over client info ======
	//send the number of connections
	connection.write_all(&u32::to_be_bytes(connections.len().try_into().unwrap_or(u32::MAX)))?;
	for i in 0..connections.len(){
		//send timestamp
		let timestamp: u64 = Local::now().timestamp().try_into().unwrap_or(0);
		connection.write_all(&mut u64::to_be_bytes(timestamp))?;
		//send name
		send_msg(&mut connection,connections[i].name.clone())?;
	}
	//====== let client select socket ======
	let mut selected_buffer = [0; 4];
	connection.read_exact(&mut selected_buffer)?;
	let selected_connection = u32::from_be_bytes(selected_buffer);
	//====== send the socket ======
	println!("ipc connection took [{:?}]",connections[selected_connection as usize].address);
	let socket_stream_binder = connections.swap_remove(selected_connection as usize).stream;
	let socket_fd = socket_stream_binder.as_raw_fd();
	let mut ancillary_buffer = [0; 128];
	let mut ancillary = SocketAncillary::new(&mut ancillary_buffer);
	ancillary.add_fds(&[socket_fd]);
	let data = io::IoSlice::new("Ok".as_ref());
	connection.send_vectored_with_ancillary(&[data],&mut ancillary)?;
	Ok(())
}
fn send_msg<T: Write>(connection: &mut T, message: String) -> io::Result<()>{
	let bytes = (message + "\x04").into_bytes();
	connection.write_all(&bytes)
}
