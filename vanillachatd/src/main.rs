use std::io;
use std::io::Read;
use std::time::Duration;
use std::thread;
use std::sync::Mutex;
use std::cell::RefCell;
use std::net::{TcpListener, TcpStream, SocketAddr};

pub struct Connection {
	stream: TcpStream,
	address: SocketAddr,
	message_buffer: String,
}

fn main() -> io::Result<()>{
	let mut connections: Vec<Connection> = vec![];
	//===== setup the listener ======
	let port: u16 = 9567;
	let addr = SocketAddr::from(([0,0,0,0],port));
	let listener = TcpListener::bind(addr)?;
	//nonblocking
	listener.set_nonblocking(true).expect("could not set listener to nonblocking");
	loop{
		//====== accept tcp connections ======
		let _ = match listener.accept(){
			Ok(connection) => handle_connection(&mut connections,connection.0,connection.1),
			Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(()),
			Err(e) => panic!("Error: {e}"),
		};
		//====== accept ipc connections ======
		//====== receive any messages ======
		for i in 0..connections.len(){
			let message = recv_msg(connections.get_mut(i).unwrap());
			if message.is_some(){
				println!("new message: {}",message.unwrap());
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
fn handle_connection(connections: &mut Vec<Connection>, stream: TcpStream, address: SocketAddr) -> Result<(), io::Error>{
	println!("New connection: {}",address);
	let connection = Connection {
		stream: stream,
		address: address,
		message_buffer: "".to_string(),
	};
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
		Err(e) => false,
	}
}
fn recv_msg(connection: &mut Connection) -> Option<String>{
	//switch to nonblocking
	connection.stream.set_nonblocking(true).expect("could not place connection socket into nonblocking mode");
	//====== read ======
	let mut buffer = [0; 1];
	let return_value = loop{
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
