use termios::*;
use std::os::fd::AsRawFd;
use std::io;
use std::thread;
use std::process::ExitCode;
use std::sync::{Arc,Mutex};
use std::cell::RefCell;

pub struct ThreadedIO {
	io_lock: Mutex<()>,
	input_buffer: Mutex<RefCell<Vec<char>>>,
	current_prompt_state: Mutex<RefCell<String>>,
	old_term_settings: Termios,
	stdin: io::Stdin,
}
impl ThreadedIO {
	fn new() -> ThreadedIO{
		let instance = ThreadedIO {
			io_lock: Mutex::new(()),
			input_buffer: Mutex::new(RefCell::new(vec![])),
			current_prompt_state: Mutex::new(RefCell::new("".to_string())),
			old_term_settings: Termios::from_fd(io::stdin().as_raw_fd()).unwrap(),
			stdin: io::stdin(),
		};
		//====== setup raw stdin ======
		let mut term = instance.old_term_settings.clone();
		term.c_lflag &= !(ICANON | ECHO);
		term.c_cc[VMIN] = 0;
		term.c_cc[VTIME] = 0;
		tcsetattr(io::stdin().as_raw_fd(),TCSANOW,&term).unwrap();
		//return
		instance
	}
	fn println(&self,string: String){
		let _io_guard = self.io_lock.lock();
		println!("{}",string);
	}
	fn input(&self,prompt: &str) -> Result<String,String>{
		Ok("".to_string())
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
fn main() -> ExitCode{
	let threaded_io_instance = ThreadedIO::new();
	let io_controller = Arc::new(threaded_io_instance);
	{//====== receiving messages thread ======
		let io = io_controller.clone();
		thread::spawn(move ||{
		});
	}
	{//====== input handling thread ======
		let io = io_controller.clone();
		match thread::spawn(move ||{
			loop {
				let message = io.input(">>>").unwrap();
				io.println(format!("you typed: [{}]",message));
			}
		}).join() {
			Ok(_) => ExitCode::SUCCESS,
			Err(e) => {println!("{:?}",e); ExitCode::SUCCESS}
		}
	}
}
