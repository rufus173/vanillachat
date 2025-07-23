#![feature(unix_socket_ancillary_data)]
use termios::*;
use chrono::{DateTime,Local};
use std::env;
use std::os::fd::AsRawFd;
use std::io;
use std::io::{Read,Write,ErrorKind};
use std::thread;
use std::sync::{Arc,Mutex};
use std::cell::RefCell;
use std::net::{TcpStream,TcpListener,SocketAddr,Shutdown};
use nix::poll::{poll,PollFd,PollFlags};
use nix::unistd::gethostname;
use std::os::fd::{AsFd,FromRawFd};
use std::os::unix::net::{SocketAncillary,UnixStream,AncillaryData};

pub struct ThreadedIO {
	io_lock: Mutex<()>,
	input_buffer: Mutex<RefCell<Vec<char>>>,
	current_prompt_state: Mutex<RefCell<String>>,
	old_term_settings: Termios,
	interupt: Mutex<bool>,
}

pub struct Connection {
	time: DateTime<Local>,
	stream: TcpStream,
	name: String,
}

pub struct AvailableConnection {
	time: DateTime<Local>,
	name: String,
}

pub struct Args {
	short: Vec<String>,
	long: Vec<String>,
	other: Vec<String>,
}

const SOCKET_LOCATION: &str = "/tmp/vanillachatd.socket";

impl Args {
	fn gather() -> Args{
		let mut args = Args {
			short: vec![],
			long: vec![],
			other: vec![],
		};
		for enumeration in env::args().enumerate(){ 
			if enumeration.0 == 0 {continue};
			let arg = enumeration.1.clone();
			//args stop after "--"
			if arg == *"--"{
				args.other.append(&mut env::args()
					.collect::<Vec<String>>()[enumeration.0+1..]
					.to_vec()
				);
				break;
			} 
			if arg.len() >= 2 && arg[..2] == *"--"{
				//long
				args.long.push(arg[2..].to_string());
			}else if arg.len() >= 1 && arg[..1] == *"-" && arg.len() != 1{
				//short
				args.short.extend(arg[1..].to_string().chars().map(|ch| ch.to_string()));
			}else{
				//other
				args.other.push(arg);
			}
		}
		//println!("short: {:?}",args.short);
		//println!("long: {:?}",args.long);
		//println!("other: {:?}",args.other);
		args
	}
}

impl ThreadedIO {
	fn new() -> ThreadedIO{
		let instance = ThreadedIO {
			io_lock: Mutex::new(()),
			input_buffer: Mutex::new(RefCell::new(vec![])),
			current_prompt_state: Mutex::new(RefCell::new("".to_string())),
			old_term_settings: Termios::from_fd(io::stdin().as_raw_fd()).unwrap(),
			interupt: Mutex::new(false),
		};
		//====== setup raw stdin ======
		let mut term = instance.old_term_settings.clone();
		term.c_lflag &= !(ICANON | ECHO); //unbuffered no echo
		term.c_cc[VMIN] = 1; //get at least one byte before read returns
		term.c_cc[VTIME] = 0; //dont wait for bytes
		tcsetattr(io::stdin().as_raw_fd(),TCSANOW,&term).unwrap();
		//return
		instance
	}
	fn println(&self,string: String) -> Result<(),std::io::Error>{
		let _io_guard = self.io_lock.lock();
		let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
		let current_prompt_state = current_prompt_state_binding.borrow();
		let mut stdout = io::stdout();
		//delete old prompt and insert line
		stdout.write_all(format!("\r\x1b[2K{}\n",string).as_bytes())?;
		//redisplay the prompt
		stdout.write_all(current_prompt_state.as_bytes())?;
		stdout.flush()?;
		Ok(())
	}
	fn input(&self,prompt: &str) -> Result<String,std::io::Error>{
		{//reset interupt
			*self.interupt.lock().unwrap() = false;
		}

		let input_buffer_binding = self.input_buffer.lock().unwrap();
		let mut input_buffer = input_buffer_binding.borrow_mut();
		{//====== initialy display the prompt ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = prompt.to_string() + &input_buffer.iter().collect::<String>();
			let mut stdout = io::stdout();
			stdout.write_all(format!("\r\x1b[2K{}",current_prompt_state).as_bytes())?;
			stdout.flush()?;
		}
		//====== poll wrapper that allows interuption ======
		let wait_for_stdin = move |timeout|{
			let stdin = io::stdin();
			let mut pollfd = [PollFd::new(stdin.as_fd(),PollFlags::POLLIN)];
			//====== wait for data ======
			loop {
				if poll::<u16>(&mut pollfd,timeout)? >= 1 {break}
				if* self.interupt.lock().expect("Mutex poisoned: fatal") == true {return Err(io::Error::from(ErrorKind::Interrupted))}
			}
			io::Result::<()>::Ok(())
		};
		//====== get input bytes ======
		wait_for_stdin(50)?;
		for ch in io::stdin().bytes(){
			match ch?{
				10 => break,//enter
				127 => {input_buffer.pop(); ()}, //delete
				ch => {
					if ch >= 32 && ch <= 126{
						input_buffer.push(char::from(ch));
					}else{
						//self.println(format!("unknown char {}",ch))?;
					}
				},
			}
			//====== display the prompt closure ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = prompt.to_string() + &input_buffer.iter().collect::<String>();
			let mut stdout = io::stdout();
			stdout.write_all(format!("\r\x1b[2K{}",current_prompt_state).as_bytes())?;
			stdout.flush()?;
			//====== wait for data ======
			wait_for_stdin(50)?;
		}
		{//====== clear the input buffer ======
			let _io_guard = self.io_lock.lock();
			let current_prompt_state_binding = self.current_prompt_state.lock().unwrap();
			let mut current_prompt_state = current_prompt_state_binding.borrow_mut();
			*current_prompt_state = "".to_string();
		}
		let message = input_buffer.iter().collect();
		input_buffer.truncate(0);
		Ok(message)
	}
	fn interupt_input(&self){
		let mut lock = self.interupt.lock().unwrap();
		*lock = true;
	}
	fn reset_term(&self){
		tcsetattr(io::stdin().as_raw_fd(),TCSANOW,&self.old_term_settings).unwrap();
	}
}
impl Drop for ThreadedIO{
	fn drop(&mut self){
		self.reset_term();
	}
}
fn main() -> Result<(),io::Error>{
	let mut port: u16 = 9567;
	let address: String;
	//====== process arguments ======
	let args = Args::gather();
	let connection: Connection;
	let our_name: String = gethostname()?.into_string().unwrap_or("Unknown name".into());
	if args.long.contains(&"help".to_string()) || args.short.contains(&"h".to_string()){
		print_help();
		return Ok(());
	}
	if args.short.contains(&"s".to_string()) || args.long.contains(&"server".to_string()){
		//------ hosting ------
		if args.other.len() > 1{
			//too many arguments!!!!
			print_help();
			return Err(io::Error::new(ErrorKind::ArgumentListTooLong,"Too many arguments."));
		}else if args.other.len() == 1{
			//port provided
			port = match args.other[0].parse(){
				Ok(p) => p,
				Err(e) => {eprintln!("Failed to parse port."); return Err(io::Error::new(ErrorKind::Other,format!("{:?}",e)))},
			};
		}
		connection = socket_from_listen_addr("0.0.0.0".into(),port,&our_name)?
	}else{
		//------ connecting ------
		if args.other.len() == 0{
			println!("using daemon's connections...");
			//get connection from socket
			connection = socket_from_daemon()?;
		}else if args.other.len() > 2{
			//too many arguments!!!!
			print_help();
			return Err(io::Error::new(ErrorKind::ArgumentListTooLong,"Too many arguments."));
		}else if args.other.len() == 1{
			//address only
			address = args.other[0].clone();
			connection = socket_from_addr(address,port,&our_name)?;
		}else{
			//address and port provided
			address = args.other[0].clone();
			port = match args.other[1].parse(){
				Ok(p) => p,
				Err(e) => {eprintln!("Failed to parse port."); return Err(io::Error::new(ErrorKind::Other,format!("{:?}",e)))},
			};
			connection = socket_from_addr(address,port,&our_name)?;
		}
	}
	//====== extract the connection details ======
	let client_name = connection.name;
	let mut socket = connection.stream;
	println!("Connected!");
	println!("client has set their name to <{}>",client_name);
	//====== init threads ======
	let threaded_io_instance = ThreadedIO::new();
	let receiving_thread: thread::JoinHandle<io::Result<()>>;
	let sending_thread: thread::JoinHandle<io::Result<()>>;
	let io_controller = Arc::new(threaded_io_instance);
	let continue_status = Arc::new(Mutex::new(true));
	//let socket: TcpStream = ;
	{//====== receiving messages thread ======
		let continue_status = continue_status.clone();
		let io = io_controller.clone();
		let mut socket = socket.try_clone()?;
		receiving_thread = thread::spawn(move ||{
			match loop {//====== mainloop ======
				let message = match recv_msg(&mut socket){
					Ok(m) => m,
					Err(e) => {
						let _ = io.println(format!("Connection error: {:?}",e))?;
						break Err(e)
					},
				};
				let _ = match io.println(format!("({client_name}) {message}")){
					Ok(()) => io::Result::Ok(()),
					Err(e) => break Err(e),
				};
				{//check if we should continue
					let keep_going = match continue_status.lock(){
						Ok(t) => t,
						Err(e) => break Err(io::Error::other(format!("{:?}",e)))
					};
					if *keep_going == false {break Ok(())}
				}
			}{//====== match result from loop ======
				Ok(()) => Ok(()),
				Err(e) => {
					let mut keep_going = match continue_status.lock(){
						Ok(t) => t,
						Err(e) => return Err(io::Error::other(format!("{:?}",e)))
					};
					*keep_going = false;
					io.interupt_input();
					Err(e)
				},
			}
		});
	}
	{//====== input handling thread ======
		let continue_status = continue_status.clone();
		let io = io_controller.clone();
		sending_thread = thread::spawn(move ||{
			match loop {//====== mainloop ======
				//get the message
				let message = match io.input(">>>"){
					Ok(m) => m,
					Err(e) => break Err(e),
				};
				//exit
				if message == "/exit" {
					let mut keep_going = match continue_status.lock(){
						Ok(t) => t,
						Err(e) => return Err(io::Error::other(format!("{:?}",e)))
					};
					//stop cleanly
					*keep_going = false;
					//kill the socket so we dont hang on recv
					socket.shutdown(Shutdown::Both);
					//exit
					break Ok(());
				}
				//send the mesage
				match send_msg(&mut socket,&message){
					Ok(()) => (),
					Err(e) => {
						let _ = io.println(format!("Connection error: {:?}",e))?;
						break Err(e)
					},
				};
				//echo their message back to them
				io.println(format!("({our_name}) {message}"));
				{//use bool to signal when to terminate thread
					let keep_going = match continue_status.lock(){
						Ok(t) => t,
						Err(e) => break Err(io::Error::other(format!("{:?}",e)))
					};
					if *keep_going == false {break Ok(())}
				}
			}{//====== match result from loop ======
				Ok(()) => Ok(()),
				Err(e) => {
					let mut keep_going = match continue_status.lock(){
						Ok(t) => t,
						Err(e) => return Err(io::Error::other(format!("{:?}",e)))
					};
					//stop cleanly
					*keep_going = false;
					Err(e)
				},
			}
		});
	}
	//====== join all the threads ======
	receiving_thread.join().expect("Couldnt join threads with main")?;
	sending_thread.join().expect("Couldnt join threads with main")
}
fn print_help(){
	let name = env::args().next().unwrap();
	println!("help:");
	println!("{} [options] <address> [port] OR",name);
	println!("{} [options] to connect through the daemon",name);
	println!("for hosting:");
	println!("{} [options] <\"-s\" or \"--server\"> [port]",name);
}
fn socket_from_daemon() -> io::Result<Connection>{
	let mut daemon = UnixStream::connect(SOCKET_LOCATION)?;
	//====== receive list of available connections ======
	let mut count_buffer = [0; 4];
	daemon.read_exact(&mut count_buffer)?;
	let connection_count = u32::from_be_bytes(count_buffer);
	let mut connections = vec![];
	for _i in 0..connection_count{
		//read timestamp of when connection was made
		let mut timestamp_buffer = [0; 8];
		daemon.read_exact(&mut timestamp_buffer)?;
		let timestamp = u64::from_be_bytes(timestamp_buffer);
		//push the connection
		connections.push(AvailableConnection {
			//if extracting the date fails, fallback to unix epoch
			time: DateTime::from_timestamp(timestamp as i64,0).unwrap_or(DateTime::UNIX_EPOCH).into(),
			//read the name they provide
			name: recv_msg(&mut daemon)?,
		});

	}
	//====== request socket ======
	let selected_connection = 0;
	if connection_count == 0{
		//no socket available
		Err(io::Error::other("No sockets available"))
	}else{
		//request
		let mut request_buffer: [u8; 4] = u32::to_be_bytes(selected_connection);
		daemon.write_all(&mut request_buffer)?;
		let mut buf = [0; 128];
		let slice_buf = io::IoSliceMut::new(&mut buf);
		let mut ancillary_buffer = [0; 128];
		let mut ancillary = SocketAncillary::new(&mut ancillary_buffer);
		daemon.recv_vectored_with_ancillary(&mut [slice_buf],&mut ancillary)?;
		//extract fds
		for ancillary_result in ancillary.messages(){
			if let AncillaryData::ScmRights(mut rights) = ancillary_result.unwrap(){
				return Ok(Connection {
					stream: unsafe {TcpStream::from_raw_fd(rights.next().expect("Couldnt find fd in ancillary data"))},
					time: connections[0].time,
					name: connections[0].name.clone(),
				});
			}
		}
		Err(io::Error::other("Could not find fd in ancillary data"))
	}
}
fn socket_from_addr(address: String, port: u16, our_name: &String) -> io::Result<Connection>{
	println!("Converting address \"{}\" and port \"{}\"",address,port);
	let addr_array: [u8; 4];
	addr_array = match address.split(".")
		.map(|x| x.parse::<u8>().unwrap_or(0))
		.collect::<Vec<u8>>()
		.try_into(){
			Ok(a) => a,
			Err(e) => return Err(io::Error::other(format!("Could not parse address: {:?}",e))),
	};
	let mut sock_addr: SocketAddr = SocketAddr::from((addr_array,port));
	sock_addr.set_port(port);
	println!("Attempting connection using address {}...",sock_addr);
	let mut stream = TcpStream::connect(sock_addr)?;
	//send our name
	send_msg(&mut stream,&our_name)?;
	//receive their name
	let name = recv_msg(&mut stream)?;
	Ok(Connection {stream: stream, name: name, time: Local::now()})
}
fn socket_from_listen_addr(address: String, port: u16, our_name: &String) -> io::Result<Connection>{
	println!("Converting address \"{}\" and port \"{}\"",address,port);
	let addr_array: [u8; 4];
	addr_array = match address.split(".")
		.map(|x| x.parse::<u8>().unwrap_or(0))
		.collect::<Vec<u8>>()
		.try_into(){
			Ok(a) => a,
			Err(e) => return Err(io::Error::other(format!("Could not parse address: {:?}",e))),
	};
	let mut sock_addr: SocketAddr = SocketAddr::from((addr_array,port));
	sock_addr.set_port(port);
	println!("Attempting to host using address {}...",sock_addr);
	let listener = TcpListener::bind(sock_addr);
	let mut stream = match listener?.accept(){
		Ok((sock,_addr)) => Ok(sock),
		Err(e) => Err(e),
	}?;
	//send our name
	send_msg(&mut stream,&our_name)?;
	//receive their name
	let name = recv_msg(&mut stream)?;
	Ok(Connection {stream: stream, name: name, time: Local::now()})
}
fn recv_msg<T: io::Read>(stream: &mut T) -> io::Result<String>{
	//switch to nonblocking
	//====== read ======
	let mut message_buffer = vec![];
	let mut buffer = [0; 1];
	loop{
		let count = match stream.read(&mut buffer){
			Ok(count) => count,
			Err(e) => break Err(e),
		};
		if count == 1 {
			if buffer[0] == 0x04{
				//end of transmition
				break Ok(message_buffer.into_iter().collect());
			}
			//====== append the char to the buffer ======
			let ch = char::from_u32(buffer[0] as u32).unwrap();
			message_buffer.push(ch);
		}else{
			break Err(io::Error::from(ErrorKind::UnexpectedEof));
		}
	}
}
fn send_msg<T: io::Write>(stream: &mut T,message: &String) -> io::Result<()>{
	stream.write_all((message.to_owned()+"\x04").as_bytes())?;
	Ok(())
}
